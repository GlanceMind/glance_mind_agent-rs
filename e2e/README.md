# GlanceMind Agent-rs E2E Test

端到端集成测试，验证 Scheduler + Agent-rs 完整工作流程。

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│                    Docker Compose 环境                       │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐  ┌───────────┐ │
│  │ PostgreSQL│  │   Redis   │  │ Scheduler │  │ Agent-rs  │ │
│  │   :5433   │  │   :6380   │  │           │  │           │ │
│  └─────┬─────┘  └─────┬─────┘  └─────┬─────┘  └─────┬─────┘ │
│        │              │              │              │       │
│        └──────────────┴──────────────┴──────────────┘       │
└─────────────────────────────────────────────────────────────┘
                              │
                    ┌─────────┴─────────┐
│   真实/Mock 外部 API │
│ TikHub + Facebook Mock + LaoZhang Mock │
                    └───────────────────┘
```

## 测试场景

- **TikTok**: 中国旅游关键词抓取
- **Instagram**: 话题内容抓取
- **Reddit**: 关键字帖子抓取
- **Twitter**: `twitter_tweet_id:` 直链抓取，默认走内部 TikHub mock
- **Facebook**: `facebook_post_url:` 直链抓取，供应商默认走内部 mock
- **AI 回复**: Campaign 模板要求回复尽量返回 "OK"

## 快速开始

### 1. 配置环境变量

```bash
cd e2e
cp .env.example .env
```

编辑 `.env` 填入需要的 API 密钥：

```bash
# 必需
OPENAI_API_KEY=your_openai_api_key

# E2E 默认走内部 TikHub mock，可直接保留
TIKHUB_API_KEY=mock-tikhub-key
TIKHUB_BASE_URL=http://tikhub-mock:8500

# 如需切换到真实 TikHub，再覆盖以下值
# TIKHUB_API_KEY=your_tikhub_api_key
# TIKHUB_BASE_URL=https://api.tikhub.io

# 可选: 覆盖 Facebook E2E 默认 mock
FACEBOOK_RAPIDAPI_KEY=your_facebook_rapidapi_key

# 可选 (Facebook supplier override, E2E 默认为内部 mock)
FACEBOOK_RAPIDAPI_HOST=facebook-scraper3.p.rapidapi.com
FACEBOOK_RAPIDAPI_BASE_URL=http://facebook-scraper-mock:8600

# OpenAI supplier override
# 默认会被 docker-compose 固定到 laozhang-mock，只有你手动改 compose 时才需要
OPENAI_BASE_URL=http://laozhang-mock:8100/v1
AI_MODEL=deepseek-ai/DeepSeek-V3
```

### 2. 运行测试

```bash
chmod +x scripts/*.sh
./scripts/start.sh

# 只跑 Facebook
./scripts/run_platform_test.sh facebook
```

### 3. 查看日志

```bash
# Scheduler 日志
docker-compose logs -f scheduler

# Agent-rs 日志
docker-compose logs -f agent-rs

# 所有日志
docker-compose logs -f
```

### 4. 重新运行测试

```bash
# 重置 campaign 状态
python3 scripts/reset_campaign.py

# 重新运行测试
pytest test_e2e_china_travel.py -v

# 或运行 Facebook 路径
pytest test_e2e_facebook.py -v
```

### 5. 清理环境

```bash
./scripts/cleanup.sh
```

## 环境变量说明

| 变量 | 必需 | 默认值 | 说明 |
|------|------|--------|------|
| `TIKHUB_API_KEY` | 仅 TikHub 场景强依赖 | mock-tikhub-key | TikHub API 密钥 |
| `FACEBOOK_RAPIDAPI_KEY` | Facebook 场景 | mock-facebook-key | Facebook RapidAPI 密钥 |
| `OPENAI_API_KEY` | ✅ | - | OpenAI API 密钥 |
| `TIKHUB_BASE_URL` | ❌ | http://tikhub-mock:8500 | TikHub API 地址，E2E 默认走内部 mock |
| `FACEBOOK_RAPIDAPI_HOST` | ❌ | facebook-scraper3.p.rapidapi.com | Facebook RapidAPI Host |
| `FACEBOOK_RAPIDAPI_BASE_URL` | ❌ | http://facebook-scraper-mock:8600 | Facebook RapidAPI Base URL |
| `OPENAI_BASE_URL` | 由 E2E compose 固定 | http://laozhang-mock:8100/v1 | OpenAI API 地址 |
| `AI_MODEL` | ❌ | deepseek-ai/DeepSeek-V3 | AI 模型名称 |
| `POSTGRES_PORT` | ❌ | 5433 | PostgreSQL 端口 |
| `REDIS_PORT` | ❌ | 6380 | Redis 端口 |
| `E2E_TIMEOUT` | ❌ | 300 | 测试超时时间(秒) |

## 测试用例

| 序号 | 测试项 | 说明 |
|------|--------|------|
| 01 | verify_initial_state | 验证 Campaign 初始状态为 DRAFT |
| 02 | activate_campaign | 激活 Campaign，验证预算冻结 |
| 03 | scheduler_creates_task | 等待 Scheduler 创建爬虫任务 |
| 04 | agent_processes_task | 等待 Agent-rs 处理完成 |
| 05 | videos_saved | 验证视频数据已保存 |
| 06 | comments_saved | 验证评论数据已保存 |
| 07 | ai_analysis_ok | 验证 AI 回复都是 "OK" |
| 08 | wallet_updated | 验证钱包余额变化 |
| 09 | summary | 打印测试汇总 |

Twitter 端到端路径使用内部 `tikhub-mock`，Facebook 使用内部 `facebook-scraper-mock`，评论分析默认走内部 `laozhang-mock`，因此不依赖真实 Twitter/Facebook/OpenAI 供应商即可验证 Scheduler -> Agent-rs -> PostgreSQL 的全链路。

## 目录结构

```
e2e/
├── .env.example           # 环境变量模板
├── docker-compose.yml     # Docker Compose 配置
├── Dockerfile             # Agent-rs 镜像
├── requirements.txt       # Python 依赖
├── conftest.py            # pytest 配置和 fixtures
├── test_e2e_china_travel.py  # 主测试用例
├── init-scripts/          # 数据库初始化脚本
│   ├── 01_schema.sql      # 数据库 schema
│   ├── 02_budget_procedures.sql  # 预算管理存储过程
│   ├── 03_seed_base.sql   # 基础数据 (平台、地区等)
│   ├── 04_mock_user.sql   # E2E 测试用户
│   ├── 05_mock_campaign.sql  # TikTok E2E Campaign
│   ├── 08_mock_twitter_campaign.sql  # Twitter E2E Campaign
│   └── 09_mock_facebook_campaign.sql  # Facebook E2E Campaign
├── scripts/
│   ├── start.sh           # 启动测试
│   ├── cleanup.sh         # 清理环境
│   ├── wait_ready.py      # 等待服务就绪
│   └── reset_campaign.py  # 重置 Campaign 状态
└── README.md
```

## 故障排除

### 1. Docker 构建失败

```bash
# 清理 Docker 缓存重建
docker-compose build --no-cache
```

### 2. 数据库连接失败

```bash
# 检查 PostgreSQL 容器状态
docker-compose ps postgres
docker-compose logs postgres
```

### 3. Scheduler 不创建任务

```bash
# 检查 Campaign 状态
docker-compose exec postgres psql -U aihub_user -d aihub_e2e_db -c \
  "SELECT id, name, status, is_frozen FROM gm_campaigns WHERE id = 99901"

# 检查 Scheduler 日志
docker-compose logs scheduler | tail -50
```

### 4. Agent-rs 不处理任务

```bash
# 检查 Redis 队列
docker-compose exec redis redis-cli LLEN crawler:task_queue

# 检查 Agent-rs 日志
docker-compose logs agent-rs | tail -50
```

### 5. AI API 错误

确认 API 密钥正确，并检查 API 服务可用性：

```bash
# 测试 TikHub
curl -H "Authorization: Bearer $TIKHUB_API_KEY" \
  "https://api.tikhub.io/api/v1/tiktok/app/v3/fetch_video_search_result?keyword=test&count=1"

# 测试 OpenAI
curl -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}]}' \
  "$OPENAI_BASE_URL/chat/completions"
```
