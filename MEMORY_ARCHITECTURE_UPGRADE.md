# 记忆架构升级建议 — Claude Desktop (Tauri Edition)

> 基于 MetaGPT 多智能体工作流分析生成 | 2026-06-05

---

## 一、当前架构分析

### 1.1 三层记忆体系现状

| 层级 | 模块 | 存储方式 | 检索方式 | 持久化 |
|------|------|----------|----------|--------|
| **工作流记忆** | metagpt/memory.rs | Vec + HashMap 索引 | 按 CauseBy 类型精确匹配 | 无 内存临时 |
| **知识库** | metagpt/knowledge.rs | Vec + HashMap | TF-IDF + 余弦相似度 | 无 内存临时 |
| **持久记忆** | db/memory_repo.rs | SQLite + FTS5 | 关键词全文搜索 + LIKE 回退 | SQLite WAL |

### 1.2 已有但未启用的能力

- Embedding 字段：MemoryRecord 已定义 embedding: Option<Vec<f32>> 但从未写入
- Embedding 配置：MemoryConfig 已配置 embedding_model: all-MiniLM-L6-v2 和 embedding_dimension: 384 但无实际调用
- 向量检索接口：MemoryStorage trait 的 search() 方法仅做文本匹配 未使用向量

### 1.3 核心缺陷

1. 无语义检索：所有搜索基于关键词匹配 无法理解同义不同形的查询
2. MetaGPT 记忆不持久：工作流结束后所有智能体记忆丢失 下次运行无法利用历史
3. 知识库不持久：KnowledgeBase 的 TF-IDF 索引仅在内存中 重启即丢失
4. 上下文管理粗放：ContextManager 使用固定 12000 字符滑动窗口 无智能压缩
5. 跨会话无关联：不同对话的记忆完全隔离 无法跨会话检索相关知识

---

## 二、升级方案

### Phase 1: 向量嵌入引擎（核心基础设施）

目标：让 embedding 字段真正生效 为所有记忆生成向量嵌入

新增文件：src-tauri/src/memory/embedding.rs

- 集成 fastembed-rs（Rust 原生 ONNX 推理 支持 all-MiniLM-L6-v2）
- 实现 EmbeddingEngine 结构体：
  - new(model_name) - 加载模型到内存
  - embed(text) - 单条文本嵌入
  - embed_batch(texts) - 批量嵌入
- 模型文件缓存到 data_dir/models/ 首次使用时自动下载约80MB
- 在 MemoryStorage insert 和 upsert 时自动生成 embedding

依赖：fastembed = 4

预期效果：每条记忆写入时自动附带 384 维向量 为后续语义检索奠基

### Phase 2: 向量检索引擎

目标：基于余弦相似度的语义搜索替代纯关键词搜索

修改文件：db/memory_repo.rs
新增文件：memory/vector_index.rs

- 在 SQLite 中新增 memory_vectors 表存储向量
- 实现 VectorIndex：add/search(余弦相似度 Top-K)/remove
- 修改 search_memories()：查询长度>10时用向量搜索 无结果回退 FTS5
- 384维 * 4bytes = 1.5KB/条 10000条仅需约15MB 线性扫描<50000条足够

### Phase 3: MetaGPT 记忆持久化

目标：智能体历史知识跨工作流保留

- Memory 新增 persist_to(workspace) 方法：工作流结束时序列化存入 SQLite
- Memory 新增 restore_from(workspace) 方法：工作流开始时加载历史记忆
- KnowledgeBase 改造：KnowledgeEntry 存入 SQLite TF-IDF 索引按需重建

效果：第二次运行时 PM 可以参考上次的 PRD 避免重复分析

### Phase 4: 智能上下文压缩

目标：用语义相关性替代粗暴的字符截断

- 新增 SemanticCompression 策略：embedding聚类 保留代表性消息 相似消息合并摘要
- 当消息总字符数>max_chars 时触发 保留最新3条完整 历史按语义聚类压缩

### Phase 5: 跨会话记忆检索

目标：聊天时自动检索相关历史记忆注入上下文

- 发送 LLM 请求前：提取最近3条消息embedding 检索最相似5条历史记忆 相似度>0.7
- 注入 system prompt 的 Relevant Memories 区段
- 记忆衰减：effective_importance = importance * 0.95^days_old
- 高频被检索的记忆 importance 自动+1

---

## 三、技术选型推荐

| 方案 | 优点 | 缺点 | 推荐 |
|------|------|------|------|
| fastembed-rs ONNX | 纯Rust 无外部依赖 离线可用 | 模型约80MB 首次加载慢 | 强烈推荐 |
| 外部API OpenAI Embeddings | 最高质量 | 需网络 有延迟和费用 | 备选 |
| sqlite-vss 扩展 | SQLite原生向量搜索 | Rust绑定不成熟 | 不推荐 |

推荐方案：fastembed-rs本地推理 + SQLite存储 + 线性扫描检索

---

## 四、实施优先级

| 优先级 | 阶段 | 工作量 | 价值 |
|--------|------|--------|------|
| P0 | Phase 1 向量嵌入引擎 | 2-3天 | 所有后续阶段的基础 |
| P0 | Phase 2 向量检索引擎 | 1-2天 | 语义搜索是核心体验 |
| P1 | Phase 3 MetaGPT记忆持久化 | 1天 | 工作流连续性 |
| P1 | Phase 5 跨会话记忆检索 | 1-2天 | 聊天体验质变 |
| P2 | Phase 4 智能上下文压缩 | 2-3天 | 性能优化 |

预计总工期：7-11天

---

## 五、MetaGPT 工作流验证结果

本次分析由 MetaGPT 多智能体工作流驱动 验证了以下修复：

| 修复项 | 状态 | 说明 |
|--------|------|------|
| 消息路由 PM-Architect-Engineer | 已修复 | CauseBy 订阅发布链路完整 |
| Tool Loop 超限处理 | 已修复 | MAX_ITERATIONS 25升50 超限返回累积文本 |
| SSE 事件流 | 部分修复 | 后端工作流完整运行 SSE流客户端断开需排查 |

工作流执行日志：
- ProductManager 完成 26598 chars PRD输出
- Architect 完成 7782 chars 设计文档输出
- Engineer 运行中 正在分析代码库并编写实现

本报告由 Claude Desktop MetaGPT 工作流自动生成
