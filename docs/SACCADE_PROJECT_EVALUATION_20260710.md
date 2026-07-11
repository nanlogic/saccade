# Saccade 项目评估

日期：2026-07-10

## 结论

Saccade 值得继续，但不应该把“取代 Chrome 的新浏览器”当成产品目标。
这个市场已有成熟内核、云浏览器和官方 MCP。Saccade 更有机会成为一层
`agent-safe browser control plane`：给 agent 提供脱敏事实、受策略约束的动作、
执行回执和 replay，同时让用户保留敏感信息与最终控制权。

当前项目处在“强原型、弱产品”阶段。技术价值已经得到内部测量，市场需求也有
外部证据；外部用户、标准 benchmark、正式安全评估、安装分发和付费意愿尚未
得到证明。

建议：继续投入一个有明确退出条件的 90 天产品验证周期。把开发集中在
`FORMMAX + DOCMAX`：网页大表单、PDF/文档表单、敏感字段人机接力和可验证输出。
通用浏览器功能只修阻塞这些工作流的问题。

## 评分

| 维度 | 当前分数 | 证据与限制 |
| --- | ---: | --- |
| 项目用处 | 8/10 | FORMMAX、真实 Gist/HN draft、长文章读取、MouseAccuracy 和本地游戏 reflex 都有 artifact。 |
| 功能实用价值 | 8/10 | 快速大表单、人机协作敏感字段、同一可见 tab、长文档读取和 replay 都对应真实工作。 |
| 产品完成度 | 5/10 | Servo 网站兼容性、UI、签名安装、外部用户验证和错误恢复仍影响陌生用户日常使用。 |
| 对 agent 的提高 | 7/10 | 结构化 truth、动作地图、验证回执和毫秒 reflex 有价值；尚无与 Playwright MCP、Chrome DevTools MCP 的统一成功率、token、延迟 A/B。 |
| 安全架构 | 8/10 | 敏感字段值隔离、用户控制副作用动作、无 cookie/storage 导出、脱敏 replay 已测。 |
| 安全验证 | 4/10 | 尚未跑 AgentDojo/AgentDyn；页面文本没有 prompt-injection 来源/污点标签；loopback control 没有每会话认证 secret。 |
| 推广性 | 3/10 | 当前没有根目录 README、LICENSE、配置好的 git remote、签名公测包或外部用户案例。 |
| 经济潜力 | 6/10 | 同类产品证明开发者愿意付费；Saccade 自己尚无付费访谈、LOI 或 pilot。 |
| 成为主流组件的潜力 | 7/10 | 安全 current-tab control、事实层和 replay 有清楚位置。 |
| 成为主流通用浏览器的潜力 | 2/10 | 浏览器兼容性、生态、更新速度和分发成本远高于当前团队能力。 |

原先的“实用性 6/10”混合了功能价值和产品完成度，容易产生误解。功能本身已经
很实用，当前拉低评分的是安装、兼容、分发和外部验证。综合判断仍是 `6/10` 的
当前产品状态、`8/10` 的功能价值和技术原型。

## 外部市场证据

浏览器 agent 市场已经形成四类产品：

1. 开源 agent runtime：Browser Use GitHub 约 10.4 万 stars，并提供从每月 40 美元
   起的云服务和按 step/browser-hour 计费。
2. 托管浏览器基础设施：Browserbase 的 Developer/Startup 方案为每月 20/99
   美元；公司在 2025 年宣布 4000 万美元 B 轮。Stagehand 约 2.35 万 stars。
3. 官方浏览器 MCP：Microsoft Playwright MCP 约 3.49 万 stars；Chrome DevTools
   MCP 约 4.66 万 stars。
4. 模型原生 computer use：OpenAI 和 Anthropic 都提供 screenshot、mouse、keyboard
   控制，并在官方文档中要求隔离环境、把网页内容视为不可信输入，并在高影响
   动作前保留人工确认。

这些数据证明三件事：开发者需要 agent 浏览器；有人为托管、稳定性和观测付费；
基础 browser control 已经高度竞争。Saccade 不能靠“agent 可以点击网页”获胜。

主要来源：

- [Browser Use GitHub](https://github.com/browser-use/browser-use)
- [Browser Use pricing](https://browser-use.com/pricing)
- [Browserbase pricing](https://www.browserbase.com/pricing)
- [Browserbase Series B](https://www.browserbase.com/blog/series-b-and-beyond)
- [Stagehand GitHub](https://github.com/browserbase/stagehand)
- [Playwright MCP](https://github.com/microsoft/playwright-mcp)
- [Chrome DevTools MCP](https://github.com/ChromeDevTools/chrome-devtools-mcp)
- [OpenAI computer use safety guidance](https://developers.openai.com/api/docs/guides/tools-computer-use)
- [Anthropic computer use security guidance](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)

GitHub star 数来自 2026-07-10 GitHub REST API 快照，只作为采用度信号，不代表
活跃用户或收入。

## 竞品位置

| 产品 | 强项 | Saccade 不该硬拼的部分 | Saccade 可争的位置 |
| --- | --- | --- | --- |
| Browser Use | 开源生态、agent loop、cloud、stealth | 通用 agent 框架、反 bot 基础设施 | 本地敏感数据隔离、可验证动作与 replay |
| Browserbase/Stagehand | 托管 Chrome、并发、proxy、观测、企业合规 | 云 browser-hour 和大规模基础设施 | 本地/私有部署的 policy control plane |
| Playwright MCP | Chrome 兼容、accessibility snapshot、成熟自动化 | 通用网页兼容和测试生态 | 更窄、更脱敏、更少 agent 可见数据的 truth |
| Chrome DevTools MCP | live Chrome、调试、性能和网络信息 | DevTools 能力广度 | 默认拒绝敏感数据、明确人机 ownership |
| OpenAI/Anthropic computer use | 模型训练、视觉推理、产品分发 | 模型能力和用户入口 | 模型无关的本地执行、安全策略和审计 |
| BrowserGym/AgentLab | 标准任务、可复现 benchmark | benchmark 生态本身 | 作为被测 runtime，证明速度/安全优势 |

Chrome DevTools MCP 的 README 明确提醒：MCP client 可以检查、调试和修改浏览器
中的数据，用户不应把不想暴露的敏感信息放进该浏览器。Saccade 的“用户看全部，
agent 只看获准且脱敏的事实”正好针对这个缺口。这是目前最可信的产品差异。

## 已经证明的价值

本地 artifact 支持以下 claim：

- FORMMAX：96 行、2 页、672 个普通字段完成，3 个敏感字段保留给用户，回执和
  value-leak 检查通过。
- 真实人机流程：用户登录，agent 在同一 session 填 Gist/Hacker News draft，
  用户检查，agent 不提交。
- current-tab：用户明确 grant 后，MCP 在同一可见 tab 获得脱敏 truth/actions、
  安全 act、导航和 replay。
- reflex：本地游戏 gate 有 1292/1292 readback、176 个 semantic facts 和 53 个
  action receipts；原始 MouseAccuracy 页面能被操作。
- public reading：The Rookies 长文章抽取 9392 chars；USCIS 页面超时时会进入
  no-cookie public HTTP fallback，不伪装成功。
- compatibility route：Servo 被 Cloudflare 阻塞时，显式 Chrome compatibility
  route 能加载 Game UI Database 并继续提供脱敏 current-tab MCP。

这些结果证明 Saccade 已超出 demo。它可以完成几类真实工作，但还没有证明对
广泛网站、模型和用户都稳定。

## 对 agent 的实际提高

当前最强提升不是“模型更聪明”，而是执行环境给模型更好的反馈：

- agent 获得 action map 和 page revision，减少坐标猜测。
- 每个动作有 receipt 和结果验证，减少模型假设点击成功。
- 结构化 article/truth 可以少喂导航、广告和 UI 噪声。
- reflex path 可以处理需要高频感知/动作的任务，不必每一步等待 LLM。
- 用户和 agent 共用可见 session，省去重复登录和状态复制。

仍缺统一 A/B。现有 agent comparison 的 token 字段部分为空，不能据此宣称
Saccade 比 Chrome/Playwright 省 token。必须用同一模型、同一任务、同一成功标准
比较：完成率、wall time、LLM tokens、browser actions、重试次数和错误恢复。

## 安全评估

Saccade 的字段隔离和 ownership 模型与 OpenAI、Anthropic 的公开建议方向一致：
把页面内容视为不可信输入，在敏感数据传输和高影响动作前停下来交给用户。

但当前实现有四个发布前缺口：

1. control endpoint 只绑定 `127.0.0.1`，协议没有每会话 capability secret。同机
   恶意进程可能探测端口并调用接口。
2. truth 没有区分 user instruction、网页数据和网页中的潜在恶意 instruction。
3. 尚未用 AgentDojo、AgentDyn 或 DoomArena 跑 prompt-injection 安全 benchmark。
4. 敏感字段识别依赖 type/name/autocomplete/label 规则；复杂 custom controls、
   canvas、shadow DOM 和语义伪装可能漏判。

AgentDojo 提供 97 个任务和 629 个安全测试；OpenAI 与 Anthropic 的文档都承认
网页 prompt injection 仍需要系统隔离和人工确认。Saccade 应该把这套 benchmark
当成产品 gate，而不是只写安全原则。

来源：

- [AgentDojo, NeurIPS 2024](https://proceedings.neurips.cc/paper_files/paper/2024/hash/97091a5177d8dc64b1da8bf3e1f6fb54-Abstract-Datasets_and_Benchmarks_Track.html)
- [BrowserGym](https://github.com/ServiceNow/BrowserGym)
- [AgentLab reproducibility](https://github.com/ServiceNow/AgentLab)

## 经济利益

当前收入能力：未证明。项目没有公开下载渠道、外部用户、付费 pilot、定价实验或
企业安全材料，因此不能给收入预测。

市场愿意付费：已证明。Browserbase 和 Browser Use 都按订阅、browser-hour、
agent step、proxy 和附加功能收费。Browserbase 的融资说明资本市场也认可浏览器
基础设施需求。

最合理的商业模型：

- 免费/开源：本地 MCP、truth/action/replay 协议、fixture 与 benchmark runner。
- 付费团队版：policy packs、审计导出、集中配置、签名版本、长期支持。
- 企业版：本地或 VPC 部署、SSO、合规日志、私有 connector、security evaluation。
- 服务收入：给 agent 团队跑 browser reliability/security benchmark 和迁移评估。

不建议：自己经营与 Browserbase 正面竞争的通用云浏览器；或先做面向普通用户的
Chrome 替代品。两条路都需要 Saccade 目前没有的兼容性、基础设施、支持和分发。

## 推广性

技术故事足够强，但当前不能顺畅传播。仓库本地检查显示：没有配置 git remote，
根目录没有 README 或 LICENSE；macOS signing/notarization 仍在 backlog；公开 release
包、安装器和外部复现报告都没有完成。

最能传播的演示不是浏览器 UI，而是一组可复现对比：

1. 同一个真实表单：Playwright/Chrome DevTools MCP 与 Saccade 分别能看到什么。
2. 用户输入 SSN/信用卡后，Saccade agent truth 中没有值，但用户页面仍完整。
3. agent 填完普通字段，用户立即看到结果；提交仍由用户控制。
4. 同一任务展示成功率、token、时间和 replay，而不是剪辑视频。
5. Prompt-injection 页面尝试诱导 agent，Saccade policy gate 阻止越权动作。

这套证据适合 GitHub、Hacker News、开发者社区和安全/agent 论文。单独展示
MouseAccuracy 会吸引注意，但不能证明商业价值。

## 成为主流的条件

Saccade 有机会成为 agent runtime 的一个主流组件，前提是它保持 engine-neutral。
Servo 可以承担低延迟 truth/reflex 实验；Chrome compatibility 必须承担真实网站
覆盖。产品接口应该是 Saccade MCP/policy/replay，不应该要求用户关心底层引擎。

90 天 gate：

1. 外部用户：完成 10 次真实任务测试，至少 6 人无需作者操作代码即可跑完一条
   有价值流程。
2. benchmark：在 BrowserGym 的固定子集上与 Playwright MCP、Chrome DevTools MCP、
   Browser Use 比较成功率、token、延迟和动作数。
3. security：加入每会话 control secret，跑 AgentDojo/AgentDyn 子集，保持敏感值
   零泄漏，并记录误拦截率。
4. distribution：补 README、LICENSE、公开 remote、版本号、一条安装命令和签名
   macOS 包。
5. 付费验证：拿到 2 个付费 pilot，或至少 3 份明确写出预算、使用场景和采购条件
   的设计伙伴意向。

通过条件：Saccade 在至少一个重要维度稳定胜出，例如敏感数据暴露、动作可验证性、
token、延迟或失败恢复，并且外部用户愿意重复使用。若没有通过，应把项目收缩成
开源安全/benchmark 工具，不再投入完整浏览器产品化。

## 最终判断

这个项目有用、有技术含量，也有经济潜力。市场证据支持继续做；当前证据不支持
宣称它已经是通用浏览器、已经比 Playwright 省 token、已经解决 prompt injection，
或已经具备商业产品状态。

下一笔投入应该买来外部证据，而不是更多功能。优先级是：认证 control channel、
标准 A/B benchmark、10 个外部用户、签名 release。四项完成后，再决定是否成立
公司、开源核心，或把它作为更大 agent 产品的底层组件。
