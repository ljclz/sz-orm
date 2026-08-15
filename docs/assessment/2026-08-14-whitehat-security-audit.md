# SZ-ORM 白帽安全代码审计报告

- **审计日期**：2026-08-14
- **审计方式**：静态代码审计（白帽/防御视角）——攻击面映射 + 逐文件代码评审 + 关键发现人工复核
- **审计范围**：60 个 workspace 成员中安全敏感面：sz-orm-auth / sz-orm-crypto / sz-orm-masking / sz-orm-core（query/pool/migration 等）/ sz-orm-sqlx / sz-orm-websocket / sz-orm-graphql / sz-orm-grpc / sz-orm-mqtt / sz-orm-axum / sz-orm-queue / sz-orm-cabi / sz-orm-wasm（real_db 代理）/ sz-orm-dtx（cross_lang）/ sz-orm-lc（bidirectional_sync）/ sz-orm-swagger（reverse）
- **方法说明**：所有发现均经 `file:line` 复核（行号真实存在）；Critical/High 级发现已逐行人工验证；未运行任何代码修改
- **总体结论**：核心数据层安全基线良好（参数化查询、方言转义、JWT 算法混淆防护、常量时间比较、AES-GCM、零 unsafe），但存在 **2 个 Critical（认证令牌可预测/类型混淆）、6 个 High（签名歧义、白名单粒度、分布式事务信任边界）**，且发现 **2 处"声称已修复但实际未落地"** 的审计失败标记

---

## 一、发现汇总

| 级别 | 数量 | 核心主题 |
|------|------|----------|
| Critical | 2 | OAuth2 授权码可预测 PRNG；JWT 访问/刷新令牌类型混淆 |
| High | 6 | HMAC 签名参数走私；SQL 白名单粒度；DTX 幂等键/恢复重放/明文 Token/Unknown 悬挂 |
| Medium | 16 | WebSocket/GraphQL/gRPC 网络入口防护缺失；having() 注入面；PBKDF2 迭代次数可控；硬编码回退密钥；TOTP 爆破；RBAC 越权等 |
| Low | 14 | 锁毒化、脱敏不彻底、MQTT 通配符、迁移名转义、错误回显等 |
| Info | 7 | 密钥内存驻留、死配置、监控投毒等 |

---

## 二、Critical 发现

### C-1. OAuth2 授权码使用可预测 PRNG——**声称已修复但未落地（审计失败标记）**

- **位置**：`packages/sz-orm-auth/src/oauth2.rs:264-272`
- **证据**（已人工复核）：
  ```rust
  fn generate_code() -> String {
      use std::collections::hash_map::DefaultHasher;   // 固定密钥 SipHash
      let mut hasher = DefaultHasher::new();
      current_nanos().hash(&mut hasher);                // 种子=纳秒时间戳
      format!("{:064x}", hasher.finish())
  }
  ```
- **矛盾证据**：`packages/sz-orm-auth/Cargo.toml:33-34` 声称 *"v1.2.1 修复 Critical C-1/C-2/C-3：使用 OsRng 替代 DefaultHasher 生成 MFA 密钥/OAuth2 授权码/令牌家族 ID"*——但 `generate_code` 仍是 DefaultHasher，**C-2 修复未落地**（对照已正确修复的 `token_store.rs:470-477` OsRng 实现）。
- **影响**：授权码熵仅 64 位且种子为可预测时间戳；攻击者观察签发时刻后在 ±1s 窗口离线枚举候选码，配合 C-1 的邻接缺陷（exchange_code 无 client_secret 校验）可直接兑换任意用户令牌 → OAuth 会话劫持/账户接管。

### C-2. JWT 访问/刷新令牌类型混淆——窃取的访问令牌可无限续期

- **位置**：`packages/sz-orm-auth/src/auth.rs:206-213`（签发，access 与 refresh 使用相同 `JwtClaims` 结构）、`auth.rs:244-257`（`refresh_token` 对任何有效 JWT 一视同仁）
- **证据**（已人工复核）：
  - `auth.rs:211-213`：refresh 令牌 = `JwtClaims::new(username, exp + 86400)`，与 access 令牌仅 exp 不同，**无 token_use/typ 区分**；
  - `auth.rs:246`：`self.encoder.decode(refresh_token)?`——不校验令牌类型；
  - `auth.rs:248-253`：每次刷新用 `now + expiration` 重新签发，循环永续。
- **影响**：攻击者窃取任一 1 小时访问令牌 → 调 refresh 接口 → 拿新令牌 → 再 refresh……**无限续期**；反向也成立（refresh 令牌可直接当访问令牌）。`JwtAuthenticator::refresh_token` 完全不接入 token_store（无轮换/重放/撤销检测）。

---

## 三、High 发现

### H-1. HmacSigner 签名串无 URL 编码——参数走私（签名歧义）+ 无时间戳/Nonce（可重放）

- **位置**：`packages/sz-orm-crypto/src/lib.rs:272-283`
- **证据**（已人工复核）：`format!("{}={}", k, v)` 拼接不做 RFC 3986 编码 → `{a:"1", b:"2"}` 与 `{a:"1&b=2"}` 签名相同；签名输入不含时间戳/随机数 → 任意请求可无限重放。
- **影响**：API 签名场景中攻击者可改造参数组合而签名仍通过（逻辑绕过）；重放攻击无防护。

### H-2. WASM DB 代理 SQL 白名单粒度过粗——DML 全开 + MySQL 文件读写函数面

- **位置**：`packages/sz-orm-wasm/src/real_db/sql_whitelist.rs:59-79`（validate 仅检查首词 + forbidden 子串）、`proxy_server.rs:279-285`（白名单检查后直接执行）
- **证据**（已人工复核）：
  - 默认白名单允许 **INSERT/UPDATE/DELETE**——WASM 端可任意 `DELETE FROM users`（无 WHERE）、全表 UPDATE；
  - MySQL 特有：`SELECT * INTO OUTFILE` / `SELECT LOAD_FILE()` 是**单语句**，前缀匹配放行（若 DB 账号有 FILE 权限可写文件/读任意文件）；
  - forbidden 子串匹配误伤合法数据（`WHERE note='create account'` 被拒）；黑名单可被 `DR/**/OP` 类注释拆分绕过（MySQL 词法层重组）——仅 sqlx 默认关闭多语句限制了利用面。
- **影响**：WASM 沙箱语义被突破为"任意数据读写"，文件读写函数面扩大为代码执行前置。

### H-3. DTX 补偿/预备消息幂等键跨事务恒定 + 操作参数丢失（资金一致性）

- **位置**：`packages/sz-orm-dtx/src/cross_lang/participant.rs:57-67`（prepare）、`participant.rs:89-95`（rollback 补偿）
- **证据**（已人工复核）：`build_compensation(&resource_id, &resource_id, "deduct", &resource_id, &json!({}))`——**tx_id 与 participant_id 都传成了 resource_id**；幂等键格式 `{tx_id}:{participant_id}:{action}` 变为恒定 `{resource_id}:{resource_id}:refund`；params 恒为空对象，补偿金额/数量丢失。
- **影响**：事务 A、B 对同一资源的补偿在远端被去重丢弃 → 扣款未退回/库存未释放；远端无法按参数执行正确补偿。

### H-4. DTX 恢复流程无限重放 + 决策完全信任对端状态

- **位置**：`packages/sz-orm-dtx/src/cross_lang/recovery.rs:158-191`（recover 主流程）、`recovery.rs:230-247`（状态查询）
- **证据**（已人工复核）：`recover()` 仅 `log_store.read_pending()`（161 行），**全程无日志状态写回**——每次进程重启对同一批 pending 事务重新下发 commit/rollback；状态查询响应（`String::from_utf8` + parse）无签名/完整性保护，"Committed" 字符串即驱动全局提交决策。
- **影响**：结合 H-5（明文传输），中间人篡改 query_status 响应可把未提交事务强行推提交或回滚；恢复操作无限重放。

### H-5. DTX Token 认证默认明文 HTTP 传输

- **位置**：`packages/sz-orm-dtx/src/cross_lang/real_transport.rs:168-175`（`endpoint_url` 仅 mTLS 走 https）、`real_transport.rs:356-364`（HTTP 客户端无 TLS 字段）、`real_transport.rs:428-436`（明文发送 Bearer）
- **证据**（已人工复核）：`ParticipantAuth::Token` 自动拼 `http://`；`ReqwestHttpCallHandler` 无任何 TLS 配置项。
- **影响**：分布式事务的认证 token 与 prepare/commit/rollback 负载全部明文——中间人窃取 token（无过期机制）可伪造事务操作；篡改响应任意操纵事务结果（与 H-4 组成完整攻击链）。

### H-6. DTX Unknown 状态既不回滚也不提交——部分回滚 + 资源永久悬挂

- **位置**：`packages/sz-orm-dtx/src/cross_lang/recovery.rs:250-281`（决策）、`recovery.rs:309-311`（回滚执行）
- **证据**：`execute_global_rollback` 只对 `is_prepared() && !is_rolled_back()` 下发 rollback，**Unknown 参与者被跳过**；恢复报告却计为"已回滚"（`recovery.rs:181`）。
- **影响**：查询超时返回 Unknown 的参与者（真实状态可能已 prepare/已提交）被静默悬挂：prepare 锁定不释放；或已提交参与者被跳过而其他参与者被回滚 → 全局不一致。

---

## 四、Medium 发现（摘要，含证据位置）

| # | 发现 | 位置（file:line） |
|---|------|------|
| M-1 | WebSocket 服务端无消息大小限制 + 默认 Handler message_log 无界增长（内存 DoS） | `sz-orm-websocket/src/server.rs:138-178`、`handler.rs:206,251` |
| M-2 | WebSocket 握手无认证/Origin 校验；`authenticate()` 是演示实现且**从未被 server.rs 调用**（死代码） | `sz-orm-websocket/src/server.rs:123-130`、`handler.rs:338-348` |
| M-3 | GraphQL 真实服务器未启用 complexity/depth 限制（CPU DoS）；自带 complexity.rs 未接入 | `sz-orm-graphql/src/real_graphql.rs:203-227` |
| M-4 | gRPC 真实服务器明文 + 项目自有的 `AuthInterceptor`（lib.rs:355）**未挂载**到 tonic Server | `sz-orm-grpc/src/real_grpc.rs:129-141` |
| M-5 | `QueryBuilder::having()` 接受原始字符串直接拼入 HAVING（全库唯一残余注入面；where_cond 已移除但 having 未处理） | `sz-orm-core/src/query.rs:987-991`、渲染点 `query.rs:1422-1424,2113-2116` |
| M-6 | `QueryBuilder::select()/column()` 列名不校验不引用（ORDER BY/GROUP BY 均 quote，唯独 SELECT 列表裸拼，风格不一致） | `sz-orm-core/src/query.rs:631-633,1271-1273,1341-1345` |
| M-7 | OAuth2 token 端点不认证客户端：exchange_code 不校验 client_secret（validate_client 存在但未调用）、无 PKCE、grant_type 未校验 | `sz-orm-auth/src/oauth2.rs:218-248` |
| M-8 | Pbkdf2Hasher::verify 迭代次数来自存储串（攻击者可控，无上下限）——CPU DoS 或弱哈希接受；**kat.rs 测试固化了 c=1 被接受** | `sz-orm-crypto/src/lib.rs:233-250`、`tests/kat.rs:62-68` |
| M-9 | dist_cache 加密密钥非 UTF-8 时回退硬编码 `"default-key"`（公开已知密钥）；`from_key_str` 裸 SHA-256 无 KDF | `sz-orm-core/src/dist_cache.rs:588,597`、`sz-orm-crypto/src/lib.rs:103-106` |
| M-10 | TOTP 验证无限流/无锁定（6 位码可在线爆破）；空 base32 密钥时验证码恒为 "000000" 且被接受 | `sz-orm-auth/src/mfa.rs:138-147,205-211` |
| M-11 | RBAC action 级权限隐式降级为 `action:任意资源`——粗粒度配置即全资源越权 | `sz-orm-auth/src/authorizer.rs:212-223` |
| M-12 | Swagger 反向生成：表名/列名黑名单过滤后字符串拼接 SQL（未参数化）；PG `regclass` 跨模式读取；MySQL 未限定 `TABLE_SCHEMA` 跨库读取 | `sz-orm-swagger/src/reverse/db_schema.rs:416-510,229-231,457` |
| M-13 | TCC/Saga 补偿**失败也标记幂等**——重试被静默吞掉，补偿永久丢失 | `sz-orm-dtx/src/cross_lang/tcc.rs:96-98`、`saga.rs:74-76` |
| M-14 | 双向同步：Merge 策略 = OrmWins 别名（resolved_fields 死代码）；单向同步绕过冲突门禁（破坏性覆盖无人工确认） | `sz-orm-lc/src/bidirectional_sync.rs:334-337,527-545` |
| M-15 | JWT 密钥无最小长度校验（HS256 弱密钥可 GPU 离线爆破）；decode 无 iss 校验、无 nbf、iat 可为未来时间、无 jti | `sz-orm-auth/src/jwt.rs:100-104,127-198` |
| M-16 | WASM 代理配置语义反转：`AuthConfig::disabled()` 实际=**拒绝所有请求**（fail-closed 但与文档相反）；`RateLimitConfig.enabled` 字段从未被读取；结果集大小检查在**全量序列化之后**（内存放大） | `sz-orm-wasm/src/real_db/proxy_server.rs:239-243,255,313-319` |
| M-17 | WASM 代理把底层 DB 错误原文 + 被拒 SQL 全文回显给调用方（信息泄露） | `sz-orm-wasm/src/real_db/proxy_server.rs:282-284,307-311` |
| M-18 | DTX HTTP 响应无大小上限（恶意参与者可返回 GB 级 JSON → 协调器 OOM） | `sz-orm-dtx/src/cross_lang/real_transport.rs:391-401,464-473` |
| M-19 | DTX `block_on` 同步调用：current_thread 运行时直接 panic；multi_thread 下阻塞 worker 线程池（服务级 DoS） | `sz-orm-dtx/src/cross_lang/real_transport.rs:502-515` |

---

## 五、Low 发现（列表）

| # | 发现 | 位置 |
|---|------|------|
| L-1 | JwtKeySet/KeyManager 用 std::sync::RwLock + unwrap（锁毒化即全线崩溃）；声称的 parking_lot 修复未覆盖此处 | `sz-orm-auth/src/jwt.rs:314-345`、`sz-orm-crypto/src/lib.rs:592-644` |
| L-2 | TokenStore::cleanup 只清 tokens，families 映射无限增长（渐进 DoS） | `sz-orm-auth/src/token_store.rs:440-445` |
| L-3 | OAuth2 client_secret 比较非常量时间 | `sz-orm-auth/src/oauth2.rs:164-169` |
| L-4 | 脱敏不彻底：IPv4 保留 3 段、IPv6 仅掩最后组、无法识别输入原样返回；邮箱保留完整域名；地址保留省市前缀 | `sz-orm-masking/src/lib.rs:99-170` |
| L-5 | MFA verify 错误区分"无密钥用户"与"码错误"（用户枚举） | `sz-orm-auth/src/mfa.rs:205-211` |
| L-6 | MQTT TopicFilter 的 `From<&str>` 静默降级为未校验过滤器（非法 `a/#/b` 匹配超广范围） | `sz-orm-mqtt/src/topics.rs:50-57,65-105` |
| L-7 | MQTT 通配符可匹配 `$` 前缀系统主题（违反 MQTT 3.1.1） | `sz-orm-mqtt/src/topics.rs:65-105` |
| L-8 | migration 表名裸拼 SQL；迁移名仅拒绝 `'`/`;` 未拒绝 `\`（MySQL 反斜杠转义绕过） | `sz-orm-core/src/migration.rs:364,458-461,473-476` |
| L-9 | axum 错误响应回显内部错误原文；gRPC `Status::internal(e.to_string())` 透传 | `sz-orm-axum/src/lib.rs:64-69,135-163`、`sz-orm-grpc/src/real_grpc.rs:63,85` |
| L-10 | DTX gRPC 测试服务端无认证（pub 非 test 模块，误部署即状态机越权） | `sz-orm-dtx/src/cross_lang/real_transport.rs:565-615` |
| L-11 | DTX 参与方 desc 派生 Serialize——明文 Token/mTLS 私钥可被序列化导出 | `sz-orm-dtx/src/cross_lang/mod.rs:49-76` |
| L-12 | CABI `Layout::from_size_align(size, 8).unwrap()` 对超大 size panic（跨 FFI 边界） | `sz-orm-cabi/src/ffi_memory.rs:30` |
| L-13 | `WasmOrmSession::query_raw` 发送空 token（功能性失效，fail-closed） | `sz-orm-wasm/src/real_db/orm_session.rs:181-195` |
| L-14 | DTX 协议版本检查仅比较两个本地值（无真实协商） | `sz-orm-dtx/src/cross_lang/real_transport.rs:251-255` |

---

## 六、Info 发现

1. 密钥/令牌明文驻留内存无零化（jwt.rs:96-97、crypto lib.rs:395,548、token_store.rs:72-87）——建议 `zeroize::Zeroizing`、令牌存哈希索引
2. oauth2.rs:246 `codes.get_mut(&req.code).unwrap()` 脆弱写法
3. DTX 参与者 `latency_ms` 被直接采信进指标（可投毒监控，observability.rs:99-110）
4. DTX Registry register/heartbeat 无认证与所有权校验（registry.rs:30-49）
5. `AuthScheme::None` 无运行时禁止（sdk_contract.rs:23-25）
6. `RealTransportConfig.max_retries` 为死配置（除测试外零使用）
7. Wasm 代理 `WasmProxyServer::handle_request` 需要 `&mut self`——多连接并发下必须外部串行化（慢查询阻塞全局）

---

## 七、已核实的安全基线（未发现问题区域）

| 区域 | 核实结论 |
|------|----------|
| SQL 参数化 | `build_where_clause_with_params` 全条件类型走 `?` 占位符；IN 列表逐元素；LIMIT/OFFSET 为数值类型 |
| 方言转义 | MySQL 反斜杠+NUL 全处理；PG/SQLite/Oracle/MSSQL `''` 双写；`sql_safety::validate_identifier` 严谨 |
| JWT 算法混淆 | 签名固定用本地密钥 HMAC + `subtle` 常量时间比较；严格校验 alg=HS256/typ=JWT；base64url 拒绝 padding；**无 alg=none/RS↔HS 攻击面** |
| 随机性 | MFA 密钥/token 家族 ID 用 OsRng（对照 C-1 未修复处的正确样本）；全仓库无 thread_rng/rand::random |
| 密码学 | AES-256-GCM 每次全新随机 nonce、AAD 透传；RSA-OAEP SHA-256；PBKDF2 默认 100k 迭代/16 字节盐（缺陷仅 M-8 的 verify 边界） |
| TokenStore | 令牌轮换 + 重放检测（旧令牌复用即撤销家族）+ 家族撤销 + TOCTOU 二次检查，逻辑正确 |
| OAuth2 redirect_uri | 精确字符串匹配（RFC 6749），无开放重定向 |
| 事务 | 保存点名称白名单校验；Drop 后台回滚；隔离级别枚举化 |
| 连接池 | CAS 计数、创建超时、断路器健全；生产代码无 unwrap |
| 脱敏健壮性 | 无正则（无 ReDoS）；切片均有长度守卫；Custom 规则解析失败安全回退 |
| unsafe | 全仓库（除 sz-orm-cabi 有 SAFETY 注释的 FFI）零 unsafe |
| 已有测试基础 | `security_attacks.rs`（伪造/过期/篡改/弱密钥/畸形 token）、crypto KAT（RFC/NIST 向量）、fuzz 6 目标（firewall_bypass/identifier_safety/query_builder 等） |

---

## 八、修复优先级

| 优先级 | 项 |
|--------|----|
| **P0 立即** | C-1（OAuth2 授权码改 OsRng）、C-2（JWT token_use claim + refresh 类型校验）、H-1（HmacSigner URL 编码 + 时间戳 nonce） |
| **P1 尽快** | H-2（白名单收口：禁 DML 或参数化+只读；禁 INTO OUTFILE 关键字）、H-3（幂等键传真实 tx_id + 携带 params，一行级修复）、H-5（强制 TLS）、H-4/H-6（恢复决策持久化 + Unknown 转 Conflict）、M-5（having 参数化）、M-7（client_secret + PKCE）、M-16（enabled 语义修正） |
| **P2 排期** | M-1/M-2/M-3/M-4（网络入口防护）、M-8（PBKDF2 上下限 + 修正 kat）、M-9（删除 default-key）、M-10（TOTP 节流）、M-11（RBAC 去降级）、M-12（元数据查询参数化 + schema 限定）、M-13/M-14（幂等/同步语义） |
| **P3 跟踪** | 全部 Low/Info |

---

## 九、审计合规自检

- ✅ 所有 `file:line` 引用均经源文件人工复核（Critical/High 逐行验证，代码片段摘录于报告）
- ✅ 未修改任何源文件（纯只读审计；本报告为新文件）
- ⚠️ 未运行 cargo 命令验证编译（静态审计）；C-1 与 Cargo.toml 声明的矛盾构成审计失败标记，建议修复后重新过门禁 1~23

---

*下一步建议：按 P0 优先修复后，运行黑帽对抗性验证（见配套方案），再执行 23 道门禁。*
