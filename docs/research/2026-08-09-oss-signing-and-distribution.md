# Pillow — 开源项目的 Windows 代码签名与分发调研

Date: 2026-08-09  
Mode: **只读调研**（未改源码/配置/测试/脚本；未跑 cargo / pnpm build / 门 / E2E）  
Worker: Grok（OSS signing & distribution research）  
Controller: Claude  
Work directory: repository root `pillow`  
Prior art: `docs/research/2026-08-08-desktop-delivery-research.md` §2.2（商业采购口径；本单独立复核开源通道与 SmartScreen）

---

## Signal

**OSS-SIGNING-RESEARCH-COMPLETE**

---

## 0. 一句话结论

**首发（GitHub 开源上传、先不走微软商店）可以不签 Authenticode**：用 Release 资产 + SHA-256 校验和 + README/安装说明里的 SmartScreen「仍要运行」引导即可开场。  
**并行建议**：一旦有可复现的 GitHub Actions Windows 构建，就评估 **SignPath Foundation 免费 OSS 通道**（若资格满足则是成本最优长期路径）；地域与主体合适时再把 **Azure Artifact Signing（约 $9.99/月）** 当作付费备选。  
**EV 不是银弹**——微软当前文档明确 **EV 不再保证即时 SmartScreen 豁免**。

---

## 1. 背景与问题边界

用户决定：上传开源社区（GitHub），**先不走微软商店**。  
此前交付调研按**商业采购**估算 OV/EV 年费；开源身份可能改变结论。本单只回答：

1. 开源是否有**免费/低成本** Authenticode 通道？  
2. SmartScreen **真实行为**与 EV 当前口径？  
3. GitHub 分发形态、`winget`、未签名项目怎么做？  
4. Tauri updater 与「本地优先、数据不外发」如何取舍？  

**标注规则：** 每一条事实写 **「查证」** 或 **「推测」**；查不到就写查不到，价格与政策不凭印象定论。

**本仓库相关现状（查证，树内）：**

| 项 | 状态 |
| --- | --- |
| 许可证 | 根目录 `LICENSE` 为 **Apache License 2.0**（OSI 认可） |
| Authenticode / 签名配置 | 无（与交付调研一致） |
| `tauri-plugin-updater` | 无 |
| `bundle.active` | 关闭（交付调研） |

---

## 2. 核心问题：免费/低成本代码签名通道

### 2.1 SignPath Foundation / SignPath.io（开源计划）

| 维度 | 结论 | 标注 |
| --- | --- | --- |
| 是否免费 | **对符合条件的 OSS 项目，SignPath 服务与 Foundation 证书声明为 free of charge** | **查证**：[signpath.org](https://signpath.org/)、[terms](https://signpath.org/terms.html) |
| 证书主体 | 证书签发给 **SignPath Foundation**，不是项目法人；用户侧发布者名显示为 Foundation，并与「自仓库可验证构建」绑定 | **查证**：terms + 首页说明 |
| 微软是否承认该路径 | Microsoft Learn《Code signing options》将 **SignPath Foundation** 列为开源选项（OV 级托管流水线） | **查证**：[Code signing options](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)（页末 Open source 段，文档标注 last updated 2026-04-21） |

#### 资格条件（查证：signpath.org/terms.html）

**免费 SignPath.io 订阅（bring-your-own 证书时也适用的基础条件）：**

- 无恶意软件 / PUA  
- **OSI 认可开源许可证**，且**不得**对全部组件做「商业双许可（commercial dual-licensing）」  
- **不得含专有/非开源组件**（维护者或其关联方发布的闭源物尤其禁止）；系统库可按 GPL v3 式「System Libraries」定义纳入安装包  
- 积极维护、**已有应被签名形态的发布物**、功能在下载页/商店条目有文档  

**若使用 SignPath Foundation 自有证书，额外约束更严：**

- 只签自己维护的仓库与自己构建的二进制；上游库策略有详细规则  
- 禁止「黑客工具」类（主动扫描可利用漏洞等）；白帽诊断有边界说明  
- 尊重隐私：外传用户数据须隐私政策 + 安装时展示 + 可关  
- 须提供卸载路径；改系统配置须有警告  
- 贡献者：**SignPath 与源码托管均须 MFA**  
- 角色自组织：Authors / Reviewers / Approvers（**每次签名请求须人工批准**）  
- 主页/下载页须有 **「Code signing policy」** 段，并写明 SignPath.io / SignPath Foundation 致谢、角色、隐私声明  
- 制品元数据（product name/version 等）须在 SignPath 侧配置并强制  

**「Common misunderstandings」要点（查证）：**

- SignPath **不是 CA**，不能把证书签到项目名下；是 Foundation 以**自己的名义**让 OSS 用  
- **不是任何项目都有权获得**；需可验证声誉；可拒批，无独立仲裁  

#### 构建流程要求（查证）

| 要求 | 内容 | 来源 |
| --- | --- | --- |
| 可验证构建 | 「Binary artifacts must be built from source code in a **verifiable** way」；签名确认二进制是仓库源码（含构建脚本与 CI 配置）的有效自动构建 | terms |
| 每次发布人工批准 | 「Every release needs **manual approval** for signing」 | terms |
| GitHub 集成检查 | 构建须真实来自 GitHub workflow；制品先作为 workflow artifact 再提交签名；**对 OSS：导致签名请求的 workflow 全部 job 须跑在 GitHub-hosted agents 上** | [docs.signpath.io GitHub](https://docs.signpath.io/trusted-build-systems/github) |
| 本地构建 | OSS 路径下**不接受**「本地机器随便编一个 exe 拿去签」——与「GitHub-hosted + origin verification」硬约束冲突 | **查证（文档推断）**：由上条组合得出；**未查到**「允许 OSS 本地签名」的官方例外 |

**对 Pillow 的含义（推测 + 部分查证）：**

- Apache-2.0 **有利于**许可证条款（**查证**：树内 LICENSE）。  
- 若未来对同一代码做**商业双许可**，可能**直接失去**免费资格（**查证**：terms 明文禁止 dual-licensing for free OSS）。  
- 必须先有 **GitHub Actions 上的可复现 Windows release 构建**（GitHub-hosted runners），再申请；本地开发机构建签名**不符合** OSS 通道设计。  
- 发布者 UI 身份是 **SignPath Foundation**，不是 “Pillow Inc.”——对个人品牌/企业采购可能是利也是弊（**推测**用户观感；**查证**证书主体政策）。  
- **批不批、等多久：本单未实测申请**，不得承诺通过率。

**真实使用者示例（查证：公开页面/仓库声明，非本机验证签名）：**

- SignPath 首页展示项目含 **Flameshot、Stellarium、Git Extensions、LiteDB** 等  
- Flameshot 仓库/社区多次写明 Windows 二进制使用 SignPath.io + SignPath Foundation 证书  

---

### 2.2 Azure Artifact Signing（原 Trusted Signing）

| 维度 | 结论 | 标注 |
| --- | --- | --- |
| 现名 | **Artifact Signing**（Trusted Signing 更名；功能声明不变） | **查证**：[Azure 产品页 FAQ](https://azure.microsoft.com/en-us/products/artifact-signing/) |
| 开源/个人免费档 | **未发现**「纯 OSS 免费用」档；按 Azure 订阅计费 | **查证**：产品页定价 FAQ；**查不到** OSS 免单政策 |
| 定价（官方表述） | **Basic：** $9.99/月，含 5,000 次签名，超额 $0.005/次；**Premium：** $99.99/月，含 100,000 次，超额同 | **查证**：Azure 产品页 FAQ（与多篇第三方一致）。**注意：** 独立定价页表格在匿名抓取时可能显示 `$-` 占位，以产品 FAQ 数字为准；**非合同报价** |
| 计费起点 | 创建 Artifact Signing account 后即开始计费；**非按比例（not pro-rated）** | **查证**：Azure pricing details FAQ |
| 身份验证 | 须 identity validation（组织或个人）；个人公共身份与 Azure **计费账户类型/信息**绑定 | **查证**：[Quickstart](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart) |
| 地域资格（Public Trust） | **组织：** 美国、加拿大、欧盟、英国、澳大利亚、新西兰、日本、韩国、新加坡、瑞士、挪威、以色列等（以 Learn 当前列表为准）；**个人开发者：须位于美国或加拿大** | **查证**：同上 Quickstart Note；Code signing options 摘要为组织 US/CA/EU/UK、个人 US/CA（两页列表不完全相同处，**以 Quickstart 更完整列表为准**，并在使用前再核一次） |
| 私钥 | 云 HSM 托管，**无需 USB token** | **查证**：Learn + 产品页 |
| SmartScreen | **与 OV 相同：需累积声誉，不保证首包免警告** | **查证**：Code signing options；SmartScreen reputation 页 |
| Tauri | **官方文档已支持 Azure Artifact Signing**（`artifact-signing-cli` + `signCommand`/配置；可用 GitHub Actions 环境变量） | **查证**：[Tauri Windows Code Signing](https://v2.tauri.app/distribute/sign/windows/)（页内 Azure Artifact Signing 段，last updated 2026-07-08） |

**对 Pillow：**

- 若维护者主体/账单身份落在资格区，这是**最便宜的「自己的发布者名」付费路径**量级约 **$120/年**（**查证价 × 12**；不含税/汇率/其它 Azure 费）。  
- **无开源免单**；与 SignPath 免费路径比，优势是**证书身份是你自己**、CI 灵活度通常更高（不强制 Foundation 人工批准模型）。  
- 不在资格区的个人：**不能**把 Artifact Signing 当默认方案（**查证**地域限制）。

---

### 2.3 sigstore / cosign

| 维度 | 结论 | 标注 |
| --- | --- | --- |
| 适用场景 | 容器镜像、OCI 制品、blob/工件的**供应链签名与透明度日志**（Fulcio 短时证书 + Rekor）；OIDC（GitHub/Google/Microsoft 等）身份绑定 | **查证**：sigstore 文档生态（cosign quickstart 等） |
| 是否替代 Authenticode | **否**。Windows 安装信任链与 SmartScreen 认的是 **Authenticode / Microsoft Trusted Root**，不是 sigstore 根 | **查证（体系差异）**：sigstore 社区 issue 等讨论过与 OS 信任库不兼容；**微软 SmartScreen 文档从不将 sigstore 列为证书选项** |
| SmartScreen 认不认 | **不认**（不会因 cosign 签名而改变「未知应用」行为） | **查证（负向证据 + 机制）**：MS 证书表仅 Store / OV·EV / 自签 / 无签名；**未找到** MS 文档称 SmartScreen 消费 cosign |
| 开源价值 | 可作为**额外**完整性证明（Release 旁挂 `.sig`、SBOM、provenance），给安全敏感用户；**不解决**双击安装体验 | **推测（用途）** + **查证（不解决 SmartScreen）** |

---

### 2.4 其它开源常用/低成本路径

| 通道 | 说明 | 标注 |
| --- | --- | --- |
| **Certum「Open Source Code Signing」** | 面向 OSS 开发者的**低价** Authenticode 产品线（证书主题常为 “Open Source Developer, &lt;姓名&gt;”）；历史上曾接近免费，近年第三方博客描述为**收费**（含智能卡/云签套件，金额因 SKU/年份变化） | **查证存在**产品线（Certum 站点与多项目 wiki）；**当前标价本单未能从官方页稳定抓取完整价目**——第三方记过约 €25+ 起 / 套装更高；**不要当免费通道** |
| **传统 OV（DigiCert/Sectigo 等）** | 全球可用；HSM/token 要求（2023 后 CA/B 规则）；微软文档量级 **$150–300/年** | **查证**：MS Code signing options 区间（**非报价**） |
| **传统 EV** | 微软文档量级 **$400+/年**；**仅为躲 SmartScreen 不推荐** | **查证**：同上 |
| **微软商店 MSIX** | 商店重签，用户侧无 SmartScreen 下载警告；**本决策先不做** | **查证**：MS 文档；产品决策边界 |
| **自签** | 公开分发 ≈ 未签名；企业可用 GPO 信任 | **查证**：MS 文档 |

---

### 2.5 通道对照总表

| 通道 | 费用 | 发布者身份 | 构建约束 | SmartScreen | 适合 Pillow 何时 | 标注 |
| --- | --- | --- | --- | --- | --- | --- |
| 不签名 | $0 | Unknown | 无 | 强警告 / 企业可拦 | 首发 alpha | 查证行为 |
| SignPath Foundation | $0（资格内） | SignPath Foundation | **强制可验证 CI**（OSS：GitHub-hosted）+ 人工批准 | 有发布者名；仍需声誉 | 公开 OSS 后优先申请 | 查证政策 |
| Azure Artifact Signing | ~$9.99/月起 | 验证后的个人/组织名 | CI 友好，不强制 GHA-hosted | 同上 | 资格区内、要自有身份 | 查证价+资格 |
| Certum OSS / 传统 OV | 低价～数百美元/年 | 个人或组织 | 自建签名流水线 | 同上 | SignPath 不可用或不愿用 Foundation 名 | 价格：查证区间/不完整 |
| EV | 更高 | 组织（典型） | 同 OV + 更严身份 | **与 OV 同**（无即时豁免） | 企业采购条款要求时 | 查证 |
| sigstore/cosign | $0 | OIDC 身份 | CI 易集成 | **无效于 SmartScreen** | 附加完整性 | 查证场景 |

---

## 3. SmartScreen 的真实行为

### 3.1 未签名安装包：用户看到什么？

**查证（Microsoft Learn — SmartScreen reputation，last updated 2026-05-06）：**

| 证书类型 | 首次下载行为（文档表） |
| --- | --- |
| 无签名 | 警告：**「Windows protected your PC」**；用户须选择 **「Run anyway」** 才能运行；**企业策略可禁止继续** |
| 自签 | 与无签名相同 |
| 有效 OV/EV | 仍可能警告为 unrecognized，但会显示**已验证发布者名**；直至声誉累积 |
| Microsoft Store | 无该下载警告（商店证书） |

**点击路径（查证：广泛一致的系统 UX + 开发者说明；MS 表只写到 “Run anyway”）：**

1. 双击从互联网下载的 setup/exe（带 Mark-of-the-Web）  
2. 蓝底 SmartScreen：**Windows protected your PC** / unrecognized app  
3. 点 **More info（更多信息）**  
4. 再点 **Run anyway（仍要运行）**  

即：**通常 2 次明确点击**（More info → Run anyway），另加安装器自身的 UAC/向导。  
**推测：** 部分环境/策略下按钮布局或文案有差异；**Smart App Control** 开启时，未签名且无声誉的可执行文件可能**直接拒绝**而非可点穿（**查证**：同页 Tip — Smart App Control 可 supersede SmartScreen，且对未签名文件更严）。

### 3.2 签名之后是否立刻不再拦？

**查证（同一 MS 文档）：**

- **否。** 即使 OV/EV 有效签名，**新二进制仍可能提示 unrecognized**，直到 **file hash** 和/或 **publisher certificate** 积累足够正向声誉。  
- 声誉建立：**「没有精确阈值」**；文档写 **「可能需要数周，以及来自广泛受众的数百次干净安装（hundreds of clean installs）」**。  
- 新版本：用**同一受信任证书**签名时，**证书声誉**有机会让新文件少警告；**未签名则每个版本从零开始**。  
- **没有**面向消费者的「手动提交换 SmartScreen 白名单」机制（企业场景另议）。

### 3.3 EV 不再保证即时豁免 — 当前口径与出处

**独立复核结论：交付调研引用仍然成立。**

**查证原文要点**（[SmartScreen reputation for Windows app developers](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)）：

> **EV certificates no longer bypass SmartScreen.**  
> Years ago, signing files with an Extended Validation (EV) code signing certificate would result in positive SmartScreen reputation by default, **but this behavior no longer exists.**  
> EV certificates may matter for enterprise procurement, but they **no longer impact SmartScreen behavior.**  
> **Paying a premium for EV solely to avoid SmartScreen warnings is no longer justified.**

**相互印证（查证）：**

- [Code signing options](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)（2026-04-21）：EV 行写明 **「Same as OV since 2024 — no longer instant bypass」**，并写 **「No longer recommended specifically for SmartScreen bypass」**  
- DigiCert 等 CA 知识库亦转发 MS 口径（**二手**，以 MS Learn 为准）

**对采购决策：** 为 SmartScreen 买 EV **不划算**（查证）。EV 若仍有价值，主要在**企业采购/合规叙事**（MS 明确保留这句话）。

---

## 4. GitHub 分发形态

### 4.1 Release 资产与校验和

**查证（行业惯例 + 包管理器实践）：**

| 实践 | 作用 | 对 SmartScreen |
| --- | --- | --- |
| GitHub Releases 托管 `.exe` / `.msi` / portable zip | 主分发渠道 | 不改变；文件仍有 MotW |
| **SHA-256**（Release body 或 `SHA256SUMS`） | 防篡改、可人工/脚本校验 | **不替代**签名 |
| 签名说明 + 官方下载只认 GitHub org | 社会工程减负 | 文档层 |
| Portable zip vs 安装器 | 部分企业/用户偏好 | 两者均可触发 SmartScreen |

**建议（推测，产品向）：** 首发即提供 **SHA-256**；说明「仅从本仓库 Releases 下载」；可选后续加 sigstore 证明（高级用户），**不**替代 Authenticode。

### 4.2 winget 是否值得考虑？对签名的要求

| 点 | 结论 | 标注 |
| --- | --- | --- |
| 机制 | 社区清单进 `microsoft/winget-pkgs`；用户 `winget install` | **查证** |
| **InstallerSha256** | **强制**：下载后哈希必须与清单一致，否则安装失败 | **查证**：MS winget 文档错误码 + 社区/博文一致描述 |
| Authenticode | **EXE/MSI 路径：未发现「必须签名才能进社区源」的硬性条款**；安全主要靠哈希 + 审 PR | **查证（负向）**：SuperUser/讨论与实现强调 hash；**MSIX** 则有签名生态要求（另一条路径） |
| 是否值得 | 有稳定版本号、静默安装开关、固定 Release URL 后再做；**不是首发阻塞项** | **推测**优先级 |

**结论：** winget **值得中期做**（发现性好），**不要求**先买证书；但**要求**稳定 URL + 正确哈希 + 可维护清单。签名对 winget **加分**（企业观感/SmartScreen），**不是**清单门槛本身。

### 4.3 不签名（或弱签名）的开源项目怎么做 — 真实例子

下列为**公开信息层面的做法归纳**（**查证**仓库/新闻/讨论；**未**在本机对每个二进制跑 `signtool verify`）。

| 项目 | 做法（公开叙述） | 可学点 |
| --- | --- | --- |
| **Flameshot** | Windows 构建使用 **SignPath**；README/站点写 Code signing policy 致谢 | 免费 OSS 签名样板 |
| **Notepad++** | 商业证书过期/资格变化后，曾改用**自签**并在官网/仓库说明；用户侧仍会遇到 Defender/SmartScreen/ASR 类问题（issue 大量） | 「大项目也可能签不动」；自签对大众**帮助有限** |
| **ShareX 等** | GitHub/官网分发安装器；依赖品牌与下载量积累声誉 | 热门应用可「熬」过警告；新项目不可照搬预期 |
| **StabilityMatrix 等讨论** | 社区建议 SignPath；维护者因费用曾不签 | 用户对 OSS 未签名有一定容忍，但支持成本在 |
| **qBittorrent 等讨论** | 社区推动评估 SignPath | 说明该通道在桌面 OSS 圈可见度上升 |

**未签名时的标准文档包（综合查证 UX + 惯例）：**

1. README：仅从官方 Release 下载  
2. 校验和步骤（PowerShell `Get-FileHash` 示例）  
3. SmartScreen：**More info → Run anyway** 截图级说明  
4. 已知限制：企业机可能完全拦；Smart App Control 可能硬拒  
5. **不要**教用户关全局 Defender  

---

## 5. 自动更新与开源 / 本地优先立场

### 5.1 开源项目常见做法

| 模式 | 说明 | 标注 |
| --- | --- | --- |
| **无内置 updater** | 用户看 GitHub Release / 包管理器；隐私最好 | 常见 |
| **可选检查更新** | 默认关；设置里显式开启后才请求版本清单 | 常见于隐私敏感工具 |
| **默认检查 + 用户确认安装** | 体验好，必有出站 HTTPS | 多数商业/半商业桌面 |
| **winget/scoop/choco 更新** | 更新路径外置到包管理器 | Windows OSS 常见 |
| **商店更新** | 本决策不做 | — |

### 5.2 「默认关闭 + 用户显式开启」是否可行？

| 项 | 结论 | 标注 |
| --- | --- | --- |
| 技术可行性 | **可行**。Tauri updater 由应用主动 `check`；可不在启动路径调用；设置开关控制 | **查证**：Tauri updater API 为显式 check/download/install；权限可 deny |
| 与本地优先 | **一致**：默认无定期外联；开启后才访问 `endpoints`（TLS） | **产品对齐（推测合理）** |
| 代价 | ① 多数用户永不更新 → 安全补丁触达差；② 仍须托管 JSON/安装包 URL；③ 密钥与发布流程复杂度在「做了 updater 的那天」一次性砸下；④ 支持文档要写清「检查更新会访问哪些主机」 | **推测**产品/运营 |
| 实现注意 | 插件一旦编译进二进制，攻击面/能力面仍在；**默认关**是策略，不是威胁模型归零 | **推测**安全工程 |

**Pillow 立场建议（推测，非已定 ADR）：**

- 首发：**不要**上自动更新。  
- 中期：若上，**默认关闭 + 设置显式开启 + 隐私说明列出 endpoint 主机**；更新包仍放 GitHub Releases 静态 JSON 即可（不必自建服务器）。  

### 5.3 两套密钥：updater 签名 ≠ Authenticode

**查证（Tauri updater 文档）：**

| | **Authenticode（Windows 代码签名）** | **Tauri updater 签名** |
| --- | --- | --- |
| 目的 | OS/SmartScreen/发布者身份；安装器完整性（Windows 信任） | **应用内**验证「这次更新包确实是开发者发布的」 |
| 算法/形态 | PE/MSI Authenticode（CA 链） | **Minisign** 风格密钥对；构建产 `.sig`；公钥进 `tauri.conf` |
| 能否关闭 updater 签名 | N/A | **文档：cannot be disabled**（做 updater 就必须签更新） |
| 私钥保管 | CA/HSM/SignPath/Azure 托管或 token | **开发者自持**；丢钥 = **无法再向已安装用户发更新**；绝不能进 git |
| CI | `signtool` / Azure / SignPath | env：`TAURI_SIGNING_PRIVATE_KEY`（及 password） |
| 与 SmartScreen | 直接相关 | **无关** |

**管理要点（查证 + 工程惯例）：**

1. **两套密钥分开存**（GitHub Secrets / 密码管理器；符合仓库密钥闸）。  
2. Authenticode 解决「陌生人第一次敢不敢装」；updater 密钥解决「已安装用户下次更新是否被劫持」。  
3. 只做 GitHub 手搓发布、无 updater 时：**只需关心 Authenticode（或暂不签）+ 校验和**；不必生成 minisign 对。

---

## 6. 分档建议

| 档位 | 做什么 | 成本 | 前提 |
| --- | --- | --- | --- |
| **现在（首发）** | GitHub Releases 分发 NSIS/MSI 或 portable；**发布 SHA-256**；README 写 SmartScreen **More info → Run anyway**、仅信官方 Release、前置 WebView2/Grok 说明（见交付调研）。**Authenticode：可不签。** 同步：评估并起草 SignPath 申请材料（许可证、隐私声明句、Code signing policy 草稿、GHA Windows 可复现构建）。**不做**默认自动更新。 | **$0** 签名费；人力 = 文档 + CI | 受众偏开发者/早期用户；接受劝退率；**不**主打企业托管机 |
| **有一定下载量后** | **优先：** SignPath Foundation（若批下）把 release 流水线接到 submit-signing-request，每版人工批准。**备选：** 主体在资格区 → Azure Artifact Signing Basic（~$9.99/月）。持续同一发布者身份以攒声誉。可选：`winget-pkgs` 清单。可选：设置里 **opt-in** 更新检查（minisign 密钥上线）。 | SignPath **$0**；或 Azure **约 $120/年**；人力 = CI + 密钥运维 | 已公开、有维护节奏；**无商业 dual-license**（SignPath）；CI 在 GitHub-hosted 可复现；声誉需时间（MS：数周 + 大量干净安装） |
| **需要企业用户时** | **自有组织身份**签名（Azure 组织验证或 OV；若采购合同写 EV 再买 EV——**不为 SmartScreen**）。提供：校验和、SBOM/可选 cosign、IT 部署说明（内网分发可绕 SmartScreen）、卸载与数据目录说明。评估是否单独「企业构建/支持」通道。默认仍可保持 **更新检查 opt-in** 以贴合本地优先。 | OV **$150–300/年量级**或 Azure 订阅；EV **$400+/年量级**（MS 文档区间，**非报价**）+ 身份与法务成本 | 企业机 SmartScreen/WDAC **可能禁止「仍要运行」**；需要合同主体名 ≠ SignPath Foundation；支持 SLA |

### 与交付调研 §2.2 的关系

| 交付调研（2026-08-08） | 本单更新 |
| --- | --- |
| 公开需 Authenticode；OV/EV **推测** $100–400 / $300–600+ | **开源免费通道存在（SignPath）**——改变「必须先花钱」的首发叙事 |
| Azure Trusted Signing「未查实时价」 | **现名 Artifact Signing；官方 FAQ Basic $9.99/月** |
| EV 不再保证即时豁免（已引 MS） | **独立复核：口径仍成立，出处见 §3.3** |

---

## 7. 明确回答：首发时到底签不签？

**首发：可以不签（推荐默认策略）。**  

理由（压缩）：

1. **SmartScreen 对「新证书 + 新哈希」同样会警告**——刚签完也往往过不了「零摩擦」门槛（**查证** MS）。  
2. 开源首发受众更能接受「More info → Run anyway」**若文档写清**（**推测**转化，无数据）。  
3. SignPath **免费但有门槛**（已发布形态、可验证 CI、资格审核）——**不应阻塞**「代码先上 GitHub」。  
4. 商业 OV/EV 与 Azure **有地域/主体/费用**；在主体与 CI 未定前采购易浪费。  

**但：** 若首发当日已具备 GitHub-hosted 可复现 Windows 构建，且 SignPath **已批准**，则**应签**——边际成本低、发布者名更干净。  
**不要为了首发去买 EV。**

---

## 8. 不确定与未查到的地方

| 项 | 状态 |
| --- | --- |
| SignPath 对 Pillow **是否批、周期多长** | **未申请、未知** |
| 维护者是否位于 Azure **个人**资格区（美/加）或组织是否落在 Public Trust 列表 | **本单不知主体地理/法人** |
| Certum OSS **当前官价与云签条款** | 页面抓取不完整；仅知产品线存在 |
| 传统 CA 实时报价 | 仅用 MS 文档区间，**非询价** |
| 「数百次干净安装」的统计定义 | MS 未公开算法 |
| Smart App Control 默认开启比例 / 企业拦截止损率 | **无可靠公开统计** |
| Pillow 是否永远不做商业 dual-license | 影响 SignPath 资格；**产品决策未在本单锁定** |
| GHA Windows 上完整 Tauri 打包的分钟数与免费额度是否够 | **禁止本单 build**；未测 |
| Keeper Bridge worker boot | 本环境 `worker` 需 `parentSessionId` 返回 400；**未阻塞**只读调研本身 |

---

## 9. 推荐下一步（仍属建议，非本单实施）

1. 公开仓库时附带：安装说明 + SHA-256 + SmartScreen 点击路径。  
2. 设计 **一条** GitHub Actions `windows-latest` release 流水线（与 SignPath OSS 约束对齐）。  
3. 准备 SignPath 申请：隐私一句（本地优先：不主动外传）、Code signing policy 页、角色/MFA。  
4. 主体地理清楚后，二选一备份：**Azure Artifact Signing** vs **Certum/传统 OV**。  
5. Updater：**延后**；若做则 **opt-in** + 独立 minisign 密钥生命周期文档。  

---

## 10. 主要出处（查证用）

- SignPath Foundation: <https://signpath.org/>、<https://signpath.org/terms.html>  
- SignPath GitHub connector: <https://docs.signpath.io/trusted-build-systems/github>  
- MS SmartScreen reputation: <https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation>  
- MS Code signing options: <https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options>  
- Azure Artifact Signing product/pricing FAQ: <https://azure.microsoft.com/en-us/products/artifact-signing/>  
- Azure Artifact Signing quickstart（地域/个人）: <https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart>  
- Tauri Windows signing: <https://v2.tauri.app/distribute/sign/windows/>  
- Tauri updater: <https://v2.tauri.app/plugin/updater/>  
- 前序：`docs/research/2026-08-08-desktop-delivery-research.md`

---

## Signal (repeat)

**OSS-SIGNING-RESEARCH-COMPLETE**
