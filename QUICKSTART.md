# Starcoin Bridge 快速部署指南

## 一键重新部署

```bash
# 完整清理并重新初始化（清除所有缓存、密钥、配置）
make restart-all

# 或者分步执行：
make clean-all    # 清理所有文件和容器
make deploy       # 部署 ETH 网络
make init-bridge  # 初始化 Bridge 配置和密钥
```

## 部署流程

### 1. 部署 Ethereum 网络 (自动化)

```bash
cd /Volumes/SSD/bridge-migration/starcoin/bridge
make deploy
```

这会：
- 启动 Anvil 本地 ETH 网络 (端口 8545)
- 自动部署所有 Bridge 合约
- 提供 HTTP API 访问部署信息 (端口 8080)

### 2. 初始化 Bridge 配置 (自动化)

```bash
make init-bridge
```

这会自动：
- 清空之前的所有密钥和配置
- 生成新的 Bridge Validator 密钥 (ECDSA)
- 生成新的 Bridge Client 密钥 (Ed25519)
- 从 ETH 部署信息中提取合约地址
- 生成 Bridge Server 配置文件
- 生成 Bridge Client 配置文件
- 生成环境变量文件

**生成的文件位置：**
- 密钥：`~/.starcoin/bridge_keys/`
  - `validator_0_bridge_key` - 验证器签名密钥
  - `bridge_client_key` - 客户端交易密钥
  
- 配置：`./bridge-config/`
  - `server-config.yaml` - Bridge 服务器配置
  - `client-config.yaml` - Bridge 客户端配置
  - `.env` - 环境变量（包含所有地址和密钥）
  - `eth-deployment.json` - ETH 部署信息
  - `SETUP_SUMMARY.txt` - 配置摘要

### 3. 查看部署信息

```bash
# 查看配置摘要
make bridge-info

# 查看部署状态
make status

# 查看环境变量
cat bridge-config/.env

# 测试 ETH RPC
make test-rpc
```

### 4. 启动 Starcoin 本地网络 (手动)

```bash
# 在新终端中启动 Starcoin 本地测试网
# 具体命令取决于 Starcoin 配置
```

### 5. 部署 Starcoin Bridge 合约 (待实现)

```bash
# 加载环境变量
source bridge-config/.env

# 部署 Starcoin 端合约
make deploy-starcoin
```

### 6. 注册 Bridge 委员会 (待实现)

```bash
make register
```

### 7. 测试跨链转账 (待实现)

```bash
make test-bridge
```

## 常用命令

```bash
# 查看帮助
make help

# 部署/重启
make deploy              # 部署 ETH 网络
make init-bridge         # 初始化 Bridge 配置
make restart-all         # 完整重新部署

# 查看状态
make status              # 查看部署状态
make bridge-info         # 查看配置信息
make ps                  # 查看运行中的容器

# 日志
make logs-eth            # ETH 节点日志
make logs-deployer       # 部署器日志

# 清理
make stop-all            # 停止所有服务
make clean-all           # 清理所有文件
```

## 重要文件

### 环境变量 (.env)
```bash
# Validator 信息
VALIDATOR_ETH_ADDRESS    # 验证器 ETH 地址
VALIDATOR_STARCOIN_ADDRESS    # 验证器 Starcoin 地址
VALIDATOR_PUBKEY         # 验证器公钥
VALIDATOR_ETH_PRIVKEY    # 验证器 ETH 私钥

# Client 信息
CLIENT_STARCOIN_ADDRESS       # 客户端 Starcoin 地址

# ETH 合约
ETH_RPC_URL             # http://localhost:8545
ETH_CHAIN_ID            # 31337
STARCOIN_BRIDGE_ADDRESS      # StarcoinBridge 代理合约
BRIDGE_COMMITTEE_ADDRESS # 委员会合约
BRIDGE_VAULT_ADDRESS    # 金库合约
WETH_ADDRESS            # WETH 代币
```

### Bridge Server 配置 (server-config.yaml)
- 监听端口：9191
- Metrics 端口：9184
- 连接到本地 ETH (localhost:8545) 和 Starcoin (localhost:9000)
- 自动批准治理操作（本地测试模式）

### Bridge Client 配置 (client-config.yaml)
- 连接到本地 ETH 和 Starcoin 网络
- 使用 client key 提交交易

## 故障排除

### ETH 部署失败
```bash
# 检查容器状态
docker ps -a | grep bridge

# 查看部署日志
make logs-deployer

# 重新部署
make clean-all && make deploy
```

### Bridge 初始化失败
```bash
# 确保 ETH 网络正在运行
curl http://localhost:8080/deployment.json

# 重新初始化
rm -rf bridge-config ~/.starcoin/bridge_keys
make init-bridge
```

### 端口冲突
```bash
# 检查端口占用
lsof -i :8545   # ETH RPC
lsof -i :8080   # Deployment info
lsof -i :9000   # Starcoin RPC

# 停止现有服务
make stop-all
```

## API 访问

### ETH 部署信息 API
```bash
# 完整部署信息
curl http://localhost:8080/deployment.json | jq

# 提取特定信息
curl -s http://localhost:8080/deployment.json | jq '{
  chainId: .network.chainId,
  contracts: .contracts | keys,
  starcoinBridge: .contracts.StarcoinBridge
}'
```

### ETH RPC
```bash
# 获取 ChainID
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}'

# 获取区块高度
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

## 架构说明

```
┌─────────────────────────────────────────────────────────────┐
│                     Docker Compose                          │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │  eth-node   │  │ eth-deployer │  │ deployment-info │  │
│  │   (Anvil)   │─>│   (Foundry)  │─>│     (Nginx)      │  │
│  │  :8545      │  │              │  │     :8080        │  │
│  └─────────────┘  └──────────────┘  └──────────────────┘  │
└─────────────────────────────────────────────────────────────┘
         │                                      │
         │                                      │ HTTP API
         │ RPC                                  │
         ▼                                      ▼
┌─────────────────┐                    ┌─────────────────┐
│  Bridge Server  │                    │  init-bridge.sh │
│   (starcoin-bridge-bridge)  │                    │  (自动配置)      │
│   :9191        │                    └─────────────────┘
└─────────────────┘                            │
         │                                      │
         │ RPC                                  │ 生成
         ▼                                      ▼
┌─────────────────┐                    ┌─────────────────┐
│Starcoin Network │                    │  Configuration  │
│   (starcoin)    │                    │  • Keys         │
│   :9000        │                    │  • Configs      │
└─────────────────┘                    │  • .env         │
                                        └─────────────────┘
```

## 下一步

运行 `make restart-all` 后，按照终端输出的提示操作：

1. ✅ ETH 网络已部署
2. ✅ Bridge 配置已初始化
3. ⏳ 启动 Starcoin 网络
4. ⏳ 部署 Starcoin Bridge 合约
5. ⏳ 注册 Bridge 委员会
6. ⏳ 测试跨链转账
