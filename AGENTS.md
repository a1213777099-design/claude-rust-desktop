# 项目规则

## 每次修改代码后必须执行
1. 同步更新版本号：`package.json`、`tauri.conf.json`、`Cargo.toml` 三处保持一致
2. 重启 dev 开发模式：`npm run dev:stable`（不要单独跑 `cargo build`/`cargo check`，只有 dev server 才会运行正确的二进制并热重载前端）