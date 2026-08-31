# PRD: 记忆系统架构升级 — 向量记忆架构

> **版本**: v1.0
> **状态**: 草案
> **创建日期**: 2025-06-05

---

## 1. 背景与现状分析

### 1.1 项目简介
**Claude Desktop (Tauri Edition)** 是一个基于 Tauri 2.0 框架构建的跨平台 AI 桌面助手（Rust 后端 + React 前端），定位为"下一代 AI 桌面助手"，核心卖点包括"真正的无限记忆"。

### 1.2 当前记忆架构（SQLite + FTS5）

当前已实现基本的记忆系统，由以下层级构成：

| 层级 | 组件 | 说明 |
|------|------|------|
| **抽象层** | `src/memory/storage/mod.rs` | `MemoryStorage` trait，定义统一存储接口 |
| **数据模型** | `MemoryRecord` | id, workspace_path, conversation_id, summary, tags, memory_type, importance, created_at, embedding (option) |
| **数据库层** | `src/db/memory_repo.rs` | SQLite CRUD + FTS5 全文搜索 |
| **摘要引擎** | `build_smart_summary()` | 从对话中提取决策/偏好/关键事实 |
| **配置层** | `src/memory/config.rs` | 已预留 embedding_model / embedding_dimension 字段 |
| **API 层** | `src/bridge/memory_handlers_v2.rs` | HTTP RESTful 接口 |
| **前端** | `src/components/MemoryPanel.tsx` | 记忆浏览/搜索/删除/类型筛选 |

**现有记忆类型**：`fact | preference | decision | context`

### 1.3 核心痛点

1. **❌ 无语义搜索** — FTS5 仅关键词匹配，无法理解语义相似性（如搜索"喜欢简洁"无法召回"偏好极简设计"）
2. **❌ 无向量嵌入实现** — `config.rs` 预留了 `embedding_model`/`embedding_dimension` 字段但未激活
3. **❌ 记忆无关联** — 无法基于语义相似度发现记忆之间的关联关系
4. **❌ 知识库孤岛** — `orchestration/metagpt/knowledge.rs` 中存在独立的 TF-IDF 知识库，与主记忆系统完全分离
5. **❌ 召回方式单一** — 仅按重要性+时间排序，缺乏语义召回
6. **❌ 无自动聚类** — 无法按主题自动分组记忆

---

## 2. 升级目标

### 2.1 战略目标
将**基于关键词的记忆系统**升级为**基于向量的语义记忆系统**，实现"真正的无限记忆"产品承诺。

### 2.2 量化目标

| 目标 | 描述 | 优先级 | 衡量指标 |
|------|------|--------|----------|
| G1 | 向量化存储与检索 | P0 | 记忆创建时自动生成 Embedding |
| G2 | 语义搜索 | P0 | 语义召回率(Recall@10) ≥ 0.8 |
| G3 | 混合搜索(FTS5+向量) | P1 | 综合搜索 NDCG@10 提升 ≥ 20% |
| G4 | 记忆关联推荐 | P1 | 查看记忆时推荐 ≥ 3 条语义相关记忆 |
| G5 | 自动聚类 | P2 | 聚类结果可浏览，准确率 ≥ 0.7 |
| G6 | 知识库整合 | P2 | TF-IDF 知识库迁移到统一向量接口 |

---

## 3. 用户故事

### 3.1 语义搜索
- **US-01**：用户搜索"前端配色方案"，能召回"之前讨论过 Tailwind 蓝色主题"的语义相关记忆
- **US-02**：搜索结果按语义相关度排序，而非仅按重要性/时间

### 3.2 记忆关联
- **US-03**：查看某条记忆时，自动推荐语义相关的其他记忆
- **US-04**：AI 对话时自动召回与当前问题语义相关的历史记忆

### 3.3 智能聚类
- **US-05**：记忆面板按主题聚类展示（如"项目A"、"学习Rust"、"旅行规划"）
- **US-06**：支持"记忆地图"直观展示记忆分布

### 3.4 开发者体验
- **US-07**：Embedding 服务可配置，支持切换模型
- **US-08**：向量存储与现有 SQLite 无缝集成，无需额外数据库

---

## 4. 功能需求

### 4.1 FR-01: Embedding 生成服务

| 项目 | 内容 |
|------|------|
| **模型** | 默认 `all-MiniLM-L6-v2` (384维)，可配置切换 |
| **方式** | 首选本地 ONNX Runtime 推理，备选 HTTP API 调用 |
| **缓存** | 相同文本 Embedding 结果缓存，避免重复计算 |
| **异步** | Embedding 生成异步执行，不阻塞主流程 |
| **批量** | 支持批量文本向量化 |

**新增模块**：`src/memory/embedding/mod.rs`，定义 `EmbeddingService` trait

### 4.2 FR-02: 向量存储与索引

| 项目 | 内容 |
|------|------|
| **存储** | SQLite BLOB 列存储向量（384维 f32 → ~1.5KB/条） |
| **索引** | 暴力搜索（<1万条）/ IVFFlat（≥1万条） |
| **降级** | 向量不可用时退化到纯 FTS5 |

**数据库变更**：
```sql
ALTER TABLE memories ADD COLUMN embedding BLOB;
CREATE TABLE IF NOT EXISTS memory_embeddings (
    memory_id TEXT PRIMARY KEY,
    embedding BLOB NOT NULL,
    dimension INTEGER NOT NULL,
    model TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
);
```

### 4.3 FR-03: 混合搜索

| 项目 | 内容 |
|------|------|
| **策略** | RRF（Reciprocal Rank Fusion）融合排序 |
| **权重** | 可配置 FTS5 权重 (默认 0.3) / 向量权重 (默认 0.7) |
| **流程** | 双通道召回 → RRF 融合 → Top-N 返回 |

### 4.4 FR-04: 记忆关联推荐
- 新增 `get_related(id, limit)` 方法，基于向量相似度查询 Top-K
- 相似度阈值 < 0.5 不展示
- 前端记忆详情页展示"相关记忆"区域

### 4.5 FR-05: 自动聚类
- 新增 `src/memory/clustering.rs` 模块
- 使用 K-Means 或 HDBSCAN 算法
- 聚类结果存 `memory_clusters` 表
- 前端记忆面板增加"主题分类"导航

### 4.6 FR-06: 知识库整合
- `KnowledgeBase` 改用 `MemoryStorage` trait 作为后端
- 知识条目作为 `memory_type: "knowledge"` 统一存储
- TF-IDF 作为降级方案保留

---

## 5. 技术方案

### 5.1 架构图

```
┌──────────┐   ┌───────────┐   ┌──────────────┐
│ 前端 UI   │   │ Bridge API │   │ Engine Core  │
│(MemoryPanel│◄─►│(REST/SSE) │◄─►│ (对话引擎)   │
│ Settings) │   │           │   │              │
└──────────┘   └─────┬─────┘   └──────┬───────┘
                      │                │
               ┌──────▼────────────────▼──────┐
               │      Memory Service           │
               │  (统一向量记忆服务)             │
               └──────┬────────────────┬──────┘
                      │                │
            ┌─────────▼──────┐  ┌─────▼─────────┐
            │ Embedding      │  │ Storage Layer  │
            │ Service (ONNX) │  │ SQLite + vec   │
            └────────────────┘  └───────────────┘
```

### 5.2 存储方案对比

| 方案 | 优点 | 缺点 | 决策 |
|------|------|------|------|
| **SQLite BLOB + Rust 向量搜索** | 零额外依赖，与现有架构完全集成 | 大规模性能下降 | ✅ **首选** |
| sqlite-vec 扩展 | 原生向量索引 | 需编译扩展，生态较新 | 备选 |
| Qdrant/Milvus | 专业向量数据库 | 额外服务，复杂度高 | ❌ 不推荐 |

### 5.3 Embedding 方案对比

| 方案 | 延迟 | 依赖 | 选择 |
|------|------|------|------|
| ONNX Runtime 本地推理 | 5-20ms | `ort` crate | ✅ **首选** |
| HTTP API 调用 | 50-200ms | 网络请求 | ✅ 备选 |
| 纯 Rust 推理 | 10-50ms | `candle` / `burn` | 可选 |

### 5.4 配置项扩展

```rust
// MemoryConfig 新增字段
pub embedding_provider: String,           // "local" | "openai" | "custom"
pub embedding_api_url: Option<String>,
pub embedding_api_key: Option<String>,
pub vector_search_enabled: bool,          // 默认 true
pub hybrid_search_weight_fts: f64,        // 0.3
pub hybrid_search_weight_vector: f64,     // 0.7
pub vector_index_type: String,            // "brute_force" | "ivfflat"
pub clustering_enabled: bool,             // 默认 true
pub clustering_interval_secs: u64,        // 86400 (每天)
```

---

## 6. 接受标准

### 6.1 功能标准

| ID | 标准 | 验证方法 |
|----|------|----------|
| AC-01 | 记忆创建时自动生成向量 Embedding | 插入后查询 embedding 列非空 |
| AC-02 | 语义搜索能召回语义相似但关键词不同的结果 | 搜索"暗色主题"召回"黑色背景偏好" |
| AC-03 | 混合搜索 NDCG@10 比纯 FTS5 提升 ≥ 20% | A/B 测试 |
| AC-04 | 记忆详情页展示 ≥ 3 条相关记忆 | 页面检查 |
| AC-05 | 记忆面板支持主题聚类浏览 | 聚类树可展开折叠 |
| AC-06 | 知识库搜索走向量检索 | 日志确认使用向量索引 |
| AC-07 | Embedding 服务支持本地和远程两种模式 | 切换配置后验证 |

### 6.2 性能标准

| 指标 | 目标值 |
|------|--------|
| 语义搜索延迟 (1000条记忆) | < 200ms |
| 混合搜索延迟 | < 300ms |
| 记忆创建延迟增量 | < 50ms (异步 Embedding) |
| 聚类耗时 (1000条) | < 5s (后台) |
| 数据库体积增量 | < 30% |

### 6.3 兼容性标准

| ID | 标准 |
|----|------|
| AC-08 | 现有 FTS5 搜索功能完全保留 |
| AC-09 | 旧数据无需重新处理，Embedding 延迟生成 |
| AC-10 | 所有现有 API 向后兼容 |
| AC-11 | TF-IDF 知识库接口保留，可灰度迁移 |

---

## 7. 风险与缓解

### 7.1 技术风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| R1: ONNX Runtime 编译困难 | 开发阻塞 | 备选 HTTP API 方案，CI 编译测试 |
| R2: 向量搜索性能不达标 | 体验下降 | 小规模暴力搜索，预留专业向量数据库接口 |
| R3: SQLite BLOB 性能瓶颈 | 读写慢 | 384维仅 1.5KB，独立表存储，监控告警 |
| R4: Embedding 模型质量差 | 搜索效果差 | 支持模型切换，提供人工反馈机制 |
| R5: 聚类算法不稳定 | 结果不可用 | 增量聚类，手动触发，结果可编辑 |

### 7.2 产品风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| R6: 用户不理解语义搜索 | 功能使用率低 | 搜索框提示，默认开启，示例搜索词 |
| R7: 关联推荐质量差 | 用户困惑 | 相似度阈值 < 0.5 不展示，可手动忽略 |
| R8: 数据库膨胀 | 磁盘占用超预期 | 监控压缩，PQ 量化预留 |

### 7.3 回滚策略
1. **功能回滚**：`vector_search_enabled = false` 一键回到纯 FTS5
2. **数据回滚**：Embedding 数据独立存储，回滚不影响原始记忆
3. **版本回滚**：保留迁移前 snapshot

---

## 8. 实施路线图

| 阶段 | 内容 | 预计工时 |
|------|------|----------|
| **Phase 1: 基础设施** | Embedding 服务 + 向量存储 + 暴力搜索 | 2周 |
| **Phase 2: 混合搜索** | FTS5+向量混合 + RRF 融合 + 前端增强 | 1周 |
| **Phase 3: 关联与聚类** | 记忆关联推荐 + 自动聚类 + 聚类 UI | 2周 |
| **Phase 4: 整合优化** | 知识库整合 + 性能优化 + 文档监控 | 1周 |

---

## 9. 附录

### 9.1 相关源文件

| 文件 | 用途 |
|------|------|
| `src/memory/config.rs` | 记忆系统配置（已预留 embedding 字段） |
| `src/memory/storage/mod.rs` | MemoryStorage trait 定义 |
| `src/db/memory_repo.rs` | 当前记忆 CRUD 实现 |
| `src/db/schema.rs` | 数据库表结构 |
| `src/db/migration.rs` | 数据库迁移 |
| `src/bridge/memory_handlers_v2.rs` | 记忆 HTTP API |
| `src/orchestration/metagpt/knowledge.rs` | TF-IDF 知识库（待整合） |
| `src/components/MemoryPanel.tsx` | 记忆面板 UI |

### 9.2 术语表

| 术语 | 说明 |
|------|------|
| FTS5 | SQLite 全文搜索引擎 |
| Embedding | 文本的向量化数值表示 |
| RRF | Reciprocal Rank Fusion，多路召回融合算法 |
| ANN | Approximate Nearest Neighbor，近似最近邻搜索 |
| ONNX | 开放神经网络交换格式 |
| NDCG | Normalized Discounted Cumulative Gain，排序质量指标 |
