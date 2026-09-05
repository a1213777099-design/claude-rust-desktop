//! 模型可控制的真实浏览器（headless Edge + CDP）。
//!
//! 与 computer_use（本机真机桌面）不同，这里启动一个 headless Chromium(Edge)，
//! 通过 CDP(Chrome DevTools Protocol) 注入鼠标/键盘/导航/滚动，并截图回传给模型。
//! 前端侧边栏面板通过 /api/browser/view 显示实时画面（画面即模型所见，坐标一一对应）。

use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Child;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

/// 结构化快照 JS：枚举可见可交互元素，分配稳定 ref（e1/e2/...），返回 JSON 数组。
/// ref 存在元素对象上（el.__agentRef），同页面生命周期内有效；跳转后需重新 snapshot。
const SNAPSHOT_JS: &str = r#"(() => {
  const sel = 'a,button,input,select,textarea,[role=button],[role=link],[role=tab],[role=checkbox],[role=radio],[role=menuitem],[role=option],[role=combobox],[role=searchbox],[role=textbox],[onclick],[contenteditable]';
  const nodes = Array.from(document.querySelectorAll(sel));
  const vis = [];
  for (const el of nodes) {
    if (vis.length >= 120) break;
    const r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) continue;
    const st = getComputedStyle(el);
    if (st.display === 'none' || st.visibility === 'hidden') continue;
    if (el.disabled) continue;
    vis.push(el);
  }
  window.__agentSeq = window.__agentSeq || 0;
  const out = vis.map(el => {
    if (!el.__agentRef) { window.__agentSeq += 1; el.__agentRef = 'e' + window.__agentSeq; }
    const r = el.getBoundingClientRect();
    let label = '';
    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') {
      label = el.placeholder || el.getAttribute('aria-label') || el.value || '';
    } else {
      label = (el.innerText || '').trim().split('\n').slice(0, 2).join(' ');
      if (!label) label = el.getAttribute('aria-label') || el.title || el.alt || '';
    }
    const item = {
      ref: el.__agentRef,
      tag: el.tagName.toLowerCase(),
      text: String(label).trim().slice(0, 100),
      x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height)
    };
    if (el.tagName === 'A' && el.href) item.href = el.href;
    if (el.type) item.type = el.type;
    if (el.value) item.value = String(el.value).slice(0, 80);
    if (el.type === 'checkbox') item.checked = !!el.checked;
    return item;
  });
  return JSON.stringify(out);
})()"#;

/// 找到系统最新版本的 Edge 可执行文件（Windows）。
pub fn find_edge() -> Option<String> {
    let dirs = [
        r"C:\Program Files (x86)\Microsoft\EdgeCore",
        r"C:\Program Files\Microsoft\EdgeCore",
    ];
    let mut candidates: Vec<(u64, String)> = Vec::new();
    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path().join("msedge.exe");
                if p.is_file() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let ver: u64 = name
                        .split('.')
                        .filter_map(|s| s.parse::<u64>().ok())
                        .fold(0u64, |acc, x| acc * 1000 + x);
                    candidates.push((ver, p.to_string_lossy().into_owned()));
                }
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.first().map(|(_, p)| p.clone())
}

/// CDP 连接：单个 actor 任务拥有 ws，同时处理「发指令」与「收响应」，避免 split 类型战争。
#[derive(Clone)]
struct CdpConn {
    tx: mpsc::Sender<(String, Value, oneshot::Sender<Value>)>,
}

impl CdpConn {
    async fn connect(url: &str) -> Result<CdpConn> {
        let (mut ws, _) = tokio_tungstenite::connect_async(url).await?;
        let (tx, mut rx) = mpsc::channel::<(String, Value, oneshot::Sender<Value>)>(64);
        tokio::spawn(async move {
            let mut next_id: u32 = 1;
            let mut pending: HashMap<u32, oneshot::Sender<Value>> = HashMap::new();
            loop {
                tokio::select! {
                    msg = ws.next() => {
                        match msg {
                            Some(Ok(Message::Text(t))) => {
                                if let Ok(v) = serde_json::from_str::<Value>(&t) {
                                    if let Some(id) = v.get("id").and_then(|i| i.as_u64()) {
                                        if let Some(tx_r) = pending.remove(&(id as u32)) {
                                            let _ = tx_r.send(v.get("result").cloned().unwrap_or(Value::Null));
                                        }
                                    }
                                }
                            }
                            _ => break,
                        }
                    }
                    Some((method, params, resp_tx)) = rx.recv() => {
                        let id = next_id; next_id += 1;
                        pending.insert(id, resp_tx);
                        let frame = serde_json::to_string(&json!({"id": id, "method": method, "params": params})).unwrap_or_default();
                        if ws.send(Message::Text(frame.into())).await.is_err() { break; }
                    }
                    else => break,
                }
            }
        });
        Ok(CdpConn { tx })
    }

    async fn send(&self, method: &str, params: Value) -> Result<Value> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send((method.to_string(), params, resp_tx))
            .await
            .map_err(|_| anyhow!("CDP 通道已关闭"))?;
        let v = resp_rx.await.map_err(|_| anyhow!("CDP 响应超时/连接断开"))?;
        if let Some(e) = v.get("error") {
            Err(anyhow!("CDP 错误: {}", e))
        } else {
            Ok(v)
        }
    }
}

/// headless 浏览器会话。全局单例，供工具重复调用共享。
pub struct BrowserSession {
    child: Mutex<Option<Child>>,
    conn: Mutex<Option<CdpConn>>,
}

impl BrowserSession {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            conn: Mutex::new(None),
        }
    }

    pub async fn ensure_ready(&self) -> Result<()> {
        if self.conn.lock().await.is_some() {
            return Ok(());
        }
        self.launch().await
    }

    async fn launch(&self) -> Result<()> {
        let edge = find_edge().ok_or_else(|| anyhow!("未找到系统 Microsoft Edge"))?;
        let port: u16 = 9229;
        let user_data = std::env::temp_dir().join("claude_browser_cdp");
        std::fs::create_dir_all(&user_data).ok();

        let child = std::process::Command::new(&edge)
            .args([
                "--headless=new",
                &format!("--remote-debugging-port={}", port),
                &format!("--user-data-dir={}", user_data.to_string_lossy()),
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-gpu",
                "--window-size=1280,900",
                "about:blank",
            ])
            .spawn()
            .map_err(|e| anyhow!("启动 Edge 失败: {}", e))?;
        *self.child.lock().await = Some(child);

        let mut ws_url: Option<String> = None;
        for _ in 0..60 {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let list_url = format!("http://127.0.0.1:{}/json/list", port);
            let resp = reqwest::get(&list_url).await.ok();
            let body: String = match resp {
                Some(r) => r.text().await.unwrap_or_default(),
                None => String::new(),
            };
            if !body.is_empty() {
                if let Ok(arr) = serde_json::from_str::<Vec<Value>>(&body) {
                    if let Some(t) = arr.iter().find(|t| t.get("type") == Some(&Value::String("page".into()))) {
                        if let Some(u) = t.get("webSocketDebuggerUrl").and_then(|v| v.as_str()) {
                            ws_url = Some(u.to_string());
                            break;
                        }
                    }
                }
            }
        }
        let ws_url = ws_url.ok_or_else(|| anyhow!("CDP 端点未就绪（端口 {}）", port))?;
        let mut conn = CdpConn::connect(&ws_url).await?;
        conn.send("Page.enable", json!({})).await?;
        conn.send("Runtime.enable", json!({})).await?;
        // about:blank 在 headless 下渲染为透明(合成后发黑)，强制白色背景避免“黑屏”。
        // 注意：CDP RGBA 的 a 是 0~1 浮点数，传 255 会被视为非法参数。
        let _ = conn
            .send(
                "Emulation.setDefaultBackgroundColorOverride",
                json!({ "color": { "r": 255, "g": 255, "b": 255, "a": 1.0 } }),
            )
            .await;
        // 先放进 conn 槽，再直接经该连接落起始页（不经 ensure_ready，避免 async 递归）。
        *self.conn.lock().await = Some(conn);
        let html_url = Self::start_page_url();
        {
            let guard = self.conn.lock().await;
            if let Some(c) = guard.as_ref() {
                let _ = c.send("Page.navigate", json!({ "url": html_url })).await;
            }
        }
        // 给起始页留出首帧绘制时间，避免随后的截图拿到黑色合成表面。
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        Ok(())
    }

    /// 内置起始页 URL（新标签页风格，本地 data: URL，不依赖网络）。
    fn start_page_url() -> String {
        let html = concat!(
            "<!doctype html><html><head><meta charset='utf-8'>",
            "<style>html,body{margin:0;height:100%;background:#fff;font-family:'Segoe UI',system-ui,sans-serif;display:flex;align-items:center;justify-content:center}",
            ".box{text-align:center;color:#6b7280}",
            ".box h1{font-size:22px;color:#374151;margin:0 0 8px;font-weight:600}",
            ".box p{font-size:13px;margin:4px 0}</style></head>",
            "<body><div class='box'><h1>内置浏览器</h1>",
            "<p>在聊天中让模型打开网页，画面会实时显示在这里。</p>",
            "<p>也可以直接在上方地址栏输入网址回车访问。</p></div></body></html>"
        );
        format!("data:text/html;charset=utf-8,{}", urlencoding::encode(html))
    }

    /// 导航到内置起始页（新标签页风格）。
    pub async fn navigate_home(&self) -> Result<Value> {
        let url = Self::start_page_url();
        self.navigate(&url).await
    }

    /// 获取页面可见文本（供不支持视觉的模型使用）。
    pub async fn get_page_text(&self) -> Result<String> {
        let r: Value = self
            .send(
                "Runtime.evaluate",
                json!({
                    "expression": "(function(){var t=(document.body&&document.body.innerText)||'';return t.length>4000?t.slice(0,4000):t;})()",
                    "returnByValue": true,
                }),
            )
            .await?;
        let val = r
            .pointer("/result/value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok(val)
    }

    async fn send(&self, method: &str, params: Value) -> Result<Value> {
        self.ensure_ready().await?;
        let guard = self.conn.lock().await;
        let conn = guard.as_ref().ok_or_else(|| anyhow!("browser not ready"))?;
        conn.send(method, params).await
    }

    /// 供 bridge 层使用的公开 CDP 指令入口（如前进/后退）。
    pub async fn send_cmd(&self, method: &str, params: Value) -> Result<Value> {
        self.send(method, params).await
    }

    pub async fn navigate(&self, url: &str) -> Result<Value> {
        self.send("Page.navigate", json!({ "url": url })).await
    }

    pub async fn screenshot(&self) -> Result<String> {
        let r: Value = self
            .send("Page.captureScreenshot", json!({ "format": "png" }))
            .await?;
        r.get("data")
            .and_then(|d| d.as_str())
            .map(String::from)
            .ok_or_else(|| anyhow!("截图失败: 无 data"))
    }

    pub async fn click(&self, x: i64, y: i64) -> Result<Value> {
        self.send("Input.dispatchMouseEvent", json!({ "type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1 })).await?;
        self.send("Input.dispatchMouseEvent", json!({ "type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1 })).await
    }

    pub async fn move_mouse(&self, x: i64, y: i64) -> Result<Value> {
        self.send("Input.dispatchMouseEvent", json!({ "type": "mouseMoved", "x": x, "y": y })).await
    }

    pub async fn type_text(&self, text: &str) -> Result<Value> {
        self.send("Input.insertText", json!({ "text": text })).await
    }

    pub async fn key(&self, key: &str) -> Result<Value> {
        let (code, vk, text) = map_key(key);
        let mut down = json!({
            "type": "keyDown",
            "key": key,
            "code": code,
            "windowsVirtualKeyCode": vk,
            "nativeVirtualKeyCode": vk,
        });
        if let Some(t) = text {
            down["text"] = json!(t);
        }
        self.send("Input.dispatchKeyEvent", down).await?;
        self.send(
            "Input.dispatchKeyEvent",
            json!({
                "type": "keyUp",
                "key": key,
                "code": code,
                "windowsVirtualKeyCode": vk,
                "nativeVirtualKeyCode": vk,
            }),
        )
        .await
    }

    /// 双击（供人工面板使用）。
    pub async fn dblclick(&self, x: i64, y: i64) -> Result<Value> {
        self.send(
            "Input.dispatchMouseEvent",
            json!({ "type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 2 }),
        )
        .await?;
        self.send(
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 2 }),
        )
        .await
    }

    /// 任意滚轮（正 deltaY 向下），供模型 scroll 与人工面板共用。
    pub async fn wheel_at(&self, x: i64, y: i64, dx: i64, dy: i64) -> Result<Value> {
        self.send(
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseWheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy }),
        )
        .await
    }

    /// 结构化快照：返回当前页面可交互元素列表（含稳定 ref）。
    pub async fn snapshot(&self) -> Result<Vec<Value>> {
        let res = self
            .send(
                "Runtime.evaluate",
                json!({ "expression": SNAPSHOT_JS, "returnByValue": true }),
            )
            .await?;
        let s = res
            .pointer("/result/value")
            .and_then(|v| v.as_str())
            .unwrap_or("[]");
        Ok(serde_json::from_str::<Vec<Value>>(s).unwrap_or_default())
    }

    /// 把 ref 解析为元素中心坐标（先滚动到可见区域）。ref 失效时报错提示重新 snapshot。
    pub async fn resolve_ref(&self, r: &str) -> Result<(i64, i64)> {
        let ref_json = serde_json::to_string(r)?;
        let expr = format!(
            r#"(() => {{
      const els = document.querySelectorAll('*');
      for (const el of els) {{
        if (el.__agentRef === {}) {{
          el.scrollIntoView({{ block: 'center', inline: 'center' }});
          const rc = el.getBoundingClientRect();
          return JSON.stringify({{ x: Math.round(rc.x + rc.width / 2), y: Math.round(rc.y + rc.height / 2) }});
        }}
      }}
      return '';
    }})()"#,
            ref_json
        );
        let res = self
            .send(
                "Runtime.evaluate",
                json!({ "expression": expr, "returnByValue": true }),
            )
            .await?;
        let s = res
            .pointer("/result/value")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if s.is_empty() {
            return Err(anyhow!("ref {} 已失效（页面可能已跳转），请重新 snapshot", r));
        }
        let v: Value = serde_json::from_str(s)?;
        Ok((
            v.get("x").and_then(|v| v.as_i64()).unwrap_or(0),
            v.get("y").and_then(|v| v.as_i64()).unwrap_or(0),
        ))
    }

    /// 按 ref 点击（滚动至可见 → 真实鼠标事件，兼容各种前端框架）。
    pub async fn click_ref(&self, r: &str) -> Result<Value> {
        let (x, y) = self.resolve_ref(r).await?;
        self.click(x, y).await
    }

    /// 按 ref 悬停。
    pub async fn hover_ref(&self, r: &str) -> Result<Value> {
        let (x, y) = self.resolve_ref(r).await?;
        self.move_mouse(x, y).await
    }

    /// 按 ref 填入文本（React 受控组件安全：native setter + input/change 事件），可选回车提交。
    pub async fn fill_ref(&self, r: &str, text: &str, submit: bool) -> Result<Value> {
        let ref_json = serde_json::to_string(r)?;
        let text_json = serde_json::to_string(text)?;
        let expr = format!(
            r#"(() => {{
      const els = document.querySelectorAll('*');
      for (const el of els) {{
        if (el.__agentRef === {}) {{
          el.scrollIntoView({{ block: 'center' }});
          el.focus();
          const tag = el.tagName;
          if (tag === 'INPUT' || tag === 'TEXTAREA') {{
            const proto = tag === 'INPUT' ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
            const setter = Object.getOwnPropertyDescriptor(proto, 'value').set;
            setter.call(el, {});
            el.dispatchEvent(new Event('input', {{ bubbles: true }}));
            el.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return 'ok';
          }}
          if (el.isContentEditable) {{
            el.textContent = {};
            el.dispatchEvent(new Event('input', {{ bubbles: true }}));
            return 'ok';
          }}
          return 'not-input';
        }}
      }}
      return '';
    }})()"#,
            ref_json, text_json, text_json
        );
        let res = self
            .send(
                "Runtime.evaluate",
                json!({ "expression": expr, "returnByValue": true }),
            )
            .await?;
        let s = res
            .pointer("/result/value")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match s {
            "ok" => {
                if submit {
                    self.key("Enter").await?;
                }
                Ok(json!({ "success": true, "filled": text }))
            }
            "not-input" => Err(anyhow!("ref {} 不是输入控件，无法 fill", r)),
            _ => Err(anyhow!("ref {} 已失效，请重新 snapshot", r)),
        }
    }

    /// 按 ref 选择下拉项。
    pub async fn select_ref(&self, r: &str, value: &str) -> Result<Value> {
        let ref_json = serde_json::to_string(r)?;
        let val_json = serde_json::to_string(value)?;
        let expr = format!(
            r#"(() => {{
      const els = document.querySelectorAll('*');
      for (const el of els) {{
        if (el.__agentRef === {} && el.tagName === 'SELECT') {{
          el.value = {};
          el.dispatchEvent(new Event('input', {{ bubbles: true }}));
          el.dispatchEvent(new Event('change', {{ bubbles: true }}));
          return 'ok';
        }}
      }}
      return '';
    }})()"#,
            ref_json, val_json
        );
        let res = self
            .send(
                "Runtime.evaluate",
                json!({ "expression": expr, "returnByValue": true }),
            )
            .await?;
        let s = res
            .pointer("/result/value")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if s == "ok" {
            Ok(json!({ "success": true, "selected": value }))
        } else {
            Err(anyhow!("ref {} 不是有效的 <select>（可能已失效）", r))
        }
    }

    /// 人工面板交互入口：把用户在画面上的点击/滚轮/键盘转发给真实页面。
    pub async fn interact(
        &self,
        action: &str,
        x: i64,
        y: i64,
        dx: i64,
        dy: i64,
        key: &str,
        text: &str,
    ) -> Result<Value> {
        self.ensure_ready().await?;
        match action {
            "click" => self.click(x, y).await,
            "dblclick" => self.dblclick(x, y).await,
            "wheel" => self.wheel_at(x, y, dx, dy).await,
            "key" => self.key(key).await,
            "type" => self.type_text(text).await,
            _ => Err(anyhow!("未知交互: {}", action)),
        }
    }

    pub async fn get_url(&self) -> Result<String> {
        let r: Value = self
            .send("Runtime.evaluate", json!({ "expression": "location.href", "returnByValue": true }))
            .await?;
        let val = r.pointer("/result/value").and_then(|v| v.as_str()).unwrap_or("");
        Ok(val.to_string())
    }
}

/// 全局单例会话，供工具与 /api/browser/* 共用。
pub static BROWSER: std::sync::OnceLock<Arc<BrowserSession>> = std::sync::OnceLock::new();

pub fn browser_session() -> Arc<BrowserSession> {
    BROWSER
        .get_or_init(|| Arc::new(BrowserSession::new()))
        .clone()
}

/// 把 computer 风格输入映射到浏览器动作，返回结果。
pub async fn execute_browser_action(input: Value) -> Result<Value> {
    let session = browser_session();
    let action = input["action_type"].as_str().unwrap_or("screenshot");

    let with_shot = |b64: String| json!({
        "__image_base64": b64,
        "type": "image",
        "mime": "image/png",
    });

    // 结果统一附带页面文本快照，不支持视觉的模型也能凭文本理解页面。
    async fn attach_text(session: &BrowserSession, out: &mut Value) {
        if let Ok(text) = session.get_page_text().await {
            if !text.trim().is_empty() {
                out["page_text"] = json!(text);
            }
        }
    }

    match action {
        "navigate" | "goto" => {
            let url = input["url"].as_str().ok_or_else(|| anyhow!("url required"))?;
            session.navigate(url).await?;
            session.wait_paint().await;
            let b64 = session.screenshot().await?;
            let mut out = with_shot(b64);
            out["success"] = json!(true);
            out["url"] = json!(url);
            attach_text(&session, &mut out).await;
            Ok(out)
        }
        "screenshot" => {
            session.wait_paint().await;
            let b64 = session.screenshot().await?;
            let url = session.get_url().await?;
            let mut out = with_shot(b64);
            out["url"] = json!(url);
            attach_text(&session, &mut out).await;
            Ok(out)
        }
        "click" => {
            let x = input["coordinate"]["x"].as_i64().ok_or_else(|| anyhow!("coordinate.x required"))?;
            let y = input["coordinate"]["y"].as_i64().ok_or_else(|| anyhow!("coordinate.y required"))?;
            session.click(x, y).await?;
            session.wait_paint().await;
            let b64 = session.screenshot().await?;
            let mut out = with_shot(b64);
            out["success"] = json!(true);
            attach_text(&session, &mut out).await;
            Ok(out)
        }
        "move" => {
            let x = input["coordinate"]["x"].as_i64().unwrap_or(0);
            let y = input["coordinate"]["y"].as_i64().unwrap_or(0);
            session.move_mouse(x, y).await?;
            Ok(json!({ "success": true }))
        }
        "scroll" => {
            let direction = input["direction"].as_str().unwrap_or("down");
            let amount = input.get("amount").and_then(|v| v.as_i64()).unwrap_or(600);
            let x = input["coordinate"]["x"].as_i64().unwrap_or(640);
            let y = input["coordinate"]["y"].as_i64().unwrap_or(450);
            let dy = if direction == "up" { -amount } else { amount };
            session.wheel_at(x, y, 0, dy).await?;
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            let b64 = session.screenshot().await?;
            let mut out = with_shot(b64);
            out["success"] = json!(true);
            attach_text(&session, &mut out).await;
            Ok(out)
        }
        "type" => {
            let text = input["text"].as_str().unwrap_or("");
            session.type_text(text).await?;
            Ok(json!({ "success": true, "typed": text }))
        }
        "key" => {
            let k = input["key"].as_str().ok_or_else(|| anyhow!("key required"))?;
            session.key(k).await?;
            Ok(json!({ "success": true, "key": k }))
        }
        "text" | "read" | "content" => {
            // 纯文本读取：不适合视觉模型的轻量方式。
            let text = session.get_page_text().await?;
            let url = session.get_url().await?;
            Ok(json!({ "success": true, "url": url, "page_text": text }))
        }
        "home" => {
            session.navigate_home().await?;
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let b64 = session.screenshot().await?;
            let mut out = with_shot(b64);
            out["success"] = json!(true);
            out["url"] = json!(session.get_url().await?);
            Ok(out)
        }
        // —— MCP 式结构化操作：snapshot → ref → 点击/输入/选择 ——
        "snapshot" => {
            session.wait_paint().await;
            let url = session.get_url().await?;
            let elements = session.snapshot().await?;
            Ok(json!({
                "success": true,
                "url": url,
                "elements": elements,
                "note": "每个元素含 ref。用 click_ref/fill/select/hover 按 ref 操作；ref 在页面跳转后失效需重新 snapshot。"
            }))
        }
        "click_ref" => {
            let r = input["ref"].as_str().ok_or_else(|| anyhow!("ref required"))?;
            session.click_ref(r).await?;
            session.wait_paint().await;
            let b64 = session.screenshot().await?;
            let mut out = with_shot(b64);
            out["success"] = json!(true);
            out["clicked"] = json!(r);
            attach_text(&session, &mut out).await;
            Ok(out)
        }
        "fill" => {
            let r = input["ref"].as_str().ok_or_else(|| anyhow!("ref required"))?;
            let text = input["text"].as_str().unwrap_or("");
            let submit = input["submit"].as_bool().unwrap_or(false);
            session.fill_ref(r, text, submit).await?;
            let b64 = session.screenshot().await?;
            let mut out = with_shot(b64);
            out["success"] = json!(true);
            out["filled"] = json!(r);
            attach_text(&session, &mut out).await;
            Ok(out)
        }
        "select" => {
            let r = input["ref"].as_str().ok_or_else(|| anyhow!("ref required"))?;
            let value = input["value"].as_str().ok_or_else(|| anyhow!("value required"))?;
            session.select_ref(r, value).await?;
            session.wait_paint().await;
            let b64 = session.screenshot().await?;
            let mut out = with_shot(b64);
            out["success"] = json!(true);
            attach_text(&session, &mut out).await;
            Ok(out)
        }
        "hover_ref" => {
            let r = input["ref"].as_str().ok_or_else(|| anyhow!("ref required"))?;
            session.hover_ref(r).await?;
            Ok(json!({ "success": true }))
        }
        "wait" => {
            let ms = input.get("ms").and_then(|v| v.as_i64()).unwrap_or(1000).min(5000);
            tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;
            session.wait_paint().await;
            let b64 = session.screenshot().await?;
            let mut out = with_shot(b64);
            out["success"] = json!(true);
            attach_text(&session, &mut out).await;
            Ok(out)
        }
        "url" | "load" => {
            let url = session.get_url().await?;
            Ok(json!({ "success": true, "url": url }))
        }
        _ => Ok(json!({ "error": format!("未知动作: {}", action), "is_error": true })),
    }
}

/// 供 /api/browser/view 用：返回最新画面 base64。
pub async fn capture_png() -> Result<String> {
    let session = browser_session();
    session.ensure_ready().await?;
    session.wait_paint().await;
    session.screenshot().await
}

impl BrowserSession {
    /// 等待页面完成加载并留出首帧绘制时间，避免截到黑色表面。
    async fn wait_paint(&self) {
        for _ in 0..6 {
            let ready = self
                .send(
                    "Runtime.evaluate",
                    json!({ "expression": "document.readyState", "returnByValue": true }),
                )
                .await
                .ok()
                .and_then(|r| r.pointer("/result/value").and_then(|v| v.as_str()).map(String::from))
                .unwrap_or_default();
            if ready == "complete" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        // readyState complete 后再给合成器一点时间出帧。
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
}

/// 常用键名 → (code, windowsVirtualKeyCode, text)。
/// 文本键带 text 参数（keyDown 时注入字符），功能键只发原始键事件。
fn map_key(key: &str) -> (String, i64, Option<String>) {
    match key {
        "Enter" => ("Enter".into(), 13, Some("\r".into())),
        "Backspace" => ("Backspace".into(), 8, None),
        "Delete" => ("Delete".into(), 46, None),
        "Tab" => ("Tab".into(), 9, Some("\t".into())),
        "Escape" => ("Escape".into(), 27, None),
        "ArrowUp" | "Up" => ("ArrowUp".into(), 38, None),
        "ArrowDown" | "Down" => ("ArrowDown".into(), 40, None),
        "ArrowLeft" | "Left" => ("ArrowLeft".into(), 37, None),
        "ArrowRight" | "Right" => ("ArrowRight".into(), 39, None),
        "Home" => ("Home".into(), 36, None),
        "End" => ("End".into(), 35, None),
        "PageUp" => ("PageUp".into(), 33, None),
        "PageDown" => ("PageDown".into(), 34, None),
        " " | "Space" => ("Space".into(), 32, Some(" ".into())),
        k if k.chars().count() == 1 => {
            let vk = k.chars().next().map(|c| c.to_ascii_uppercase() as i64).unwrap_or(0);
            (k.to_string(), vk, Some(k.to_string()))
        }
        k => (k.to_string(), 0, None),
    }
}