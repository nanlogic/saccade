# Agent Builder Recruitment, Wave 1

Date: 2026-07-09

## Scope

This is a public-source recruiting list for Nanmesh preflight testing. We will
contact people through a public profile link, personal site, X, or LinkedIn.
We will not post outreach under a GitHub issue, scrape private details, or send
bulk messages.

Saccade dogfood gate: the Chrome compatibility route opened the public
`openai/openai-agents-python` repository in 8.2 seconds, exposed 334 visible
actions, and let MCP attach to that same visible tab. The run did not click or
write to GitHub. Evidence:
`runs/chrome_compat_mcp/ai030b_github_agents_read/report.json`.

Each invite asks a builder to bring one current tool decision: an MCP server,
provider, database, auth layer, browser tool, or deployment integration. The
test lasts 20 minutes. We want a candid no-fit result as much as a useful one.

## Candidate Pool

| # | Candidate | Evidence | Why they fit | Public route |
| --- | --- | --- | --- | --- |
| 1 | [honysyang](https://github.com/honysyang) | [Agents SDK #3770](https://github.com/openai/openai-agents-python/issues/3770) | Proposed internals-focused examples covering MCP, sandboxing, sessions, and run configuration. | Check profile before contact |
| 2 | [houtaroy](https://github.com/houtaroy) | [Agents SDK #3738](https://github.com/openai/openai-agents-python/issues/3738) | Working on turn-aware session history. | Check profile before contact |
| 3 | [ibrhimAli](https://github.com/ibrhimAli) | [Agents SDK #3133](https://github.com/openai/openai-agents-python/issues/3133) | Reported a WebSocket authorization failure. | Check profile before contact |
| 4 | [hvppycoding](https://github.com/hvppycoding) | [Agents SDK #2671](https://github.com/openai/openai-agents-python/issues/2671) | Asked for safe state changes between agent turns. | Check profile before contact |
| 5 | [bmd1905](https://github.com/bmd1905) | [Agents SDK #3404](https://github.com/openai/openai-agents-python/issues/3404) | Proposed eager tool dispatch to reduce tool and streaming latency. | [X](https://x.com/bmd1905) |
| 6 | [tcconnally](https://github.com/tcconnally) | [Agents SDK #3662](https://github.com/openai/openai-agents-python/issues/3662) | Asked for encrypted persistent memory exposed through MCP tools. | [Website](https://perseus.observer) |
| 7 | [DouweM](https://github.com/DouweM) | [Pydantic AI PR #6344](https://github.com/pydantic/pydantic-ai/pull/6344) | Added behavior annotations and MCP annotation propagation for tool policy. | [Website](https://douwe.me) |
| 8 | [jgoerner](https://github.com/jgoerner) | [Pydantic AI #6350](https://github.com/pydantic/pydantic-ai/issues/6350) | Wants MCP resources available as model-callable tools. | Check profile before contact |
| 9 | [dsfaccini](https://github.com/dsfaccini) | [Pydantic AI #6071](https://github.com/pydantic/pydantic-ai/issues/6071) | Reproduced a DBOS stdio-MCP deadlock during workflow re-execution. | [X](https://x.com/dasfacc) |
| 10 | [axiomofjoy](https://github.com/axiomofjoy) | [Pydantic AI #6401](https://github.com/pydantic/pydantic-ai/issues/6401) | Reported hallucinated native tool calls from an Anthropic path. | [LinkedIn](https://www.linkedin.com/in/xandersong/) |
| 11 | [isheng-eqi](https://github.com/isheng-eqi) | [Pydantic AI #6370](https://github.com/pydantic/pydantic-ai/issues/6370) | Found deferred tool calls and retries disappearing after sibling failures. | [X](https://x.com/ishengeqi) |
| 12 | [dexhunter](https://github.com/dexhunter) | [Pydantic AI #6396](https://github.com/pydantic/pydantic-ai/issues/6396) | Working on streamed-response overhead. | [Website](https://dex.moe) |
| 13 | [AmirF194](https://github.com/AmirF194) | [Pydantic AI #6364](https://github.com/pydantic/pydantic-ai/issues/6364) | Reported adapter failures across Groq and Hugging Face. | [Website](https://fastinfer.org/) |
| 14 | [mikko-lab](https://github.com/mikko-lab) | [LangGraph #8314](https://github.com/langchain-ai/langgraph/issues/8314) | Reproduced mutable state corruption in router logic. | Check profile before contact |
| 15 | [shivajith](https://github.com/shivajith) | [LangGraph #8217](https://github.com/langchain-ai/langgraph/issues/8217) | Reported an interrupt propagation failure around tool calls. | Check profile before contact |
| 16 | [Bais-Huang](https://github.com/Bais-Huang) | [LangGraph #8116](https://github.com/langchain-ai/langgraph/issues/8116) | Working through Postgres checkpoint configuration. | Check profile before contact |
| 17 | [adarshprabhu03](https://github.com/adarshprabhu03) | [LangGraph #8204](https://github.com/langchain-ai/langgraph/issues/8204) | Reported a tool return-direct edge case. | Check profile before contact |
| 18 | [markpalfreyman](https://github.com/markpalfreyman) | [LangGraph #8298](https://github.com/langchain-ai/langgraph/issues/8298) | Found checkpoint loss during non-graceful exits. | [Website](https://markpalfreyman.com) |
| 19 | [MarioAlessandroNapoli](https://github.com/MarioAlessandroNapoli) | [LangGraph #7417](https://github.com/langchain-ai/langgraph/issues/7417) | Reported long tool calls re-executing from checkpoints. | [LinkedIn](https://www.linkedin.com/in/marioalessandronapoli/) |
| 20 | [Correctover](https://github.com/Correctover) | [LangGraph #8308](https://github.com/langchain-ai/langgraph/issues/8308) | Proposed governance checks for checkpointers. | [Website](https://correctover.com) |
| 21 | [makroumi](https://github.com/makroumi) | [LangGraph #7714](https://github.com/langchain-ai/langgraph/issues/7714) | Measured checkpoint storage and token overhead. | [Website](https://makroumi.hashnode.dev) |
| 22 | [AndrewOYLK](https://github.com/AndrewOYLK) | [LangGraph #7985](https://github.com/langchain-ai/langgraph/issues/7985) | Found MCP content-block compatibility trouble in ToolNode. | Check profile before contact |
| 23 | [Rul1an](https://github.com/Rul1an) | [LangGraph #8304](https://github.com/langchain-ai/langgraph/issues/8304) | Needs tool call IDs to survive human approval interrupts. | Check profile before contact |
| 24 | [edgarfloresguerra2011-a11y](https://github.com/edgarfloresguerra2011-a11y) | [CrewAI #6463](https://github.com/crewAIInc/crewAI/issues/6463) | Proposed MCP server security certification. | Check profile before contact |
| 25 | [SemeAIPletinnya](https://github.com/SemeAIPletinnya) | [CrewAI #6025](https://github.com/crewAIInc/crewAI/issues/6025) | Proposed release-control mediation before agent tool execution. | [X](https://x.com/adelayida210519) |
| 26 | [cschanhniem](https://github.com/cschanhniem) | [CrewAI #6221](https://github.com/crewAIInc/crewAI/issues/6221) | Working on deterministic tool permission gating. | [Website](https://runlumi.app) |
| 27 | [abhinaykrupa](https://github.com/abhinaykrupa) | [CrewAI #6246](https://github.com/crewAIInc/crewAI/issues/6246) | Asked for production safety documentation for local code execution. | [LinkedIn](https://linkedin.com/in/abhi-tech-leader) |
| 28 | [azender1](https://github.com/azender1) | [CrewAI #5802](https://github.com/crewAIInc/crewAI/issues/5802) | Reported duplicate side effects when tool calls retry. | Check profile before contact |
| 29 | [ritsth](https://github.com/ritsth) | [CrewAI #6449](https://github.com/crewAIInc/crewAI/issues/6449) | Reported real tool calls disappearing in a recovery path. | Check profile before contact |
| 30 | [nagasatish007](https://github.com/nagasatish007) | [CrewAI #5888](https://github.com/crewAIInc/crewAI/issues/5888) | Requested a tool authorization middleware hook. | [Website](https://tealtiger.ai) |
| 31 | [Om-Borse26](https://github.com/Om-Borse26) | [CrewAI #6417](https://github.com/crewAIInc/crewAI/issues/6417) | Reported async context truncation. | Check profile before contact |
| 32 | [molinto](https://github.com/molinto) | [CrewAI #6430](https://github.com/crewAIInc/crewAI/issues/6430) | Reported a tool-use error path that raises the wrong exception. | Check profile before contact |
| 33 | [maxpetrusenkoagent](https://github.com/maxpetrusenkoagent) | [CrewAI PR #6373](https://github.com/crewAIInc/crewAI/pull/6373) | Wrote production code-execution guidance on sandboxes and failure handling. | Check profile before contact |
| 34 | [dontgitit](https://github.com/dontgitit) | [Mastra #19203](https://github.com/mastra-ai/mastra/issues/19203) | Reported broken observability spans in a Datadog exporter. | Check profile before contact |
| 35 | [robo-trh](https://github.com/robo-trh) | [Mastra #19202](https://github.com/mastra-ai/mastra/issues/19202) | Reported reconnect and auth-refresh gaps in an agent session stream. | Check profile before contact |
| 36 | [gregorskii](https://github.com/gregorskii) | [Mastra #19200](https://github.com/mastra-ai/mastra/issues/19200) | Requested retry classification for workflows. | Check profile before contact |
| 37 | [sachinp9797](https://github.com/sachinp9797) | [Mastra #19189](https://github.com/mastra-ai/mastra/issues/19189) | Found an MCP elicitation capability mismatch. | Check profile before contact |
| 38 | [noahjohnhay](https://github.com/noahjohnhay) | [Mastra #19143](https://github.com/mastra-ai/mastra/issues/19143) | Requested on-demand search and loading for agents. | Check profile before contact |
| 39 | [sgarfinkel](https://github.com/sgarfinkel) | [Mastra #19184](https://github.com/mastra-ai/mastra/issues/19184) | Reported missing retry and inference visibility in traces. | Check profile before contact |
| 40 | [theianjones](https://github.com/theianjones) | [Mastra #14091](https://github.com/mastra-ai/mastra/issues/14091) | Reproduced a Cloudflare Workers/Vite integration break after an upgrade. | [Website](https://ianjones.info) |
| 41 | [roaminro](https://github.com/roaminro) | [Mastra PR #19193](https://github.com/mastra-ai/mastra/pull/19193) | Added MCP notification coverage for tools, resources, logs, and progress. | Check profile before contact |
| 42 | [mstysk](https://github.com/mstysk) | [Mastra #17291](https://github.com/mastra-ai/mastra/issues/17291) | Needs Hono authentication context inside streamable HTTP MCP deployments. | [X](https://x.com/mstysk) |

## First 25 Invitations

Send these only through the linked public route. Replace `[name]` with the
name shown on that route. Do not send a GitHub issue comment.

### 1. bmd1905

> Hi [name], I read your Agents SDK proposal on eager tool dispatch. I am testing Nanmesh, a preflight layer that gives an agent evidence about an integration before it recommends or installs it. Bring one live decision about a tool, provider, or MCP server and we can test it in a 20-minute call. I want to know whether it cuts a bad recommendation or adds work. I will not ask you to promote it.

### 2. tcconnally

> Hi [name], your request for encrypted persistent memory through MCP tools caught my attention. Nanmesh checks the integration record before an agent selects a tool. Would you test it against one memory or MCP decision you have in front of you? I can wire the session around your real question in 20 minutes and record a no-fit result as product feedback.

### 3. DouweM

> Hi [name], I saw your work on behavior annotations and MCP tool metadata in Pydantic AI. I am testing Nanmesh against the question agents face before they select a tool: what does this integration do, where does it fail, and what evidence supports the choice? Could you bring one tool-policy decision to a 20-minute session? I will use the result to improve the preflight record, not to ask for a review.

### 4. dsfaccini

> Hi [name], your DBOS stdio-MCP deadlock report is the kind of production edge case Nanmesh should surface before a team adopts a tool. I am looking for builders with a live integration question. Bring one MCP, workflow, or storage choice and we can test whether the preflight adds useful evidence in 20 minutes. A failed test helps me find the gap.

### 5. axiomofjoy

> Hi [name], I read your report on hallucinated native tool calls. I am testing Nanmesh, which gives an agent a compact evidence packet before it recommends a tool or provider. Would you test it on one current tool-selection or reliability question? I can join for 20 minutes and help wire the test around your own stack.

### 6. isheng-eqi

> Hi [name], your deferred tool-call failure report shows why an agent needs more than a package description before it selects an integration. I am testing Nanmesh with builders who have a real decision in progress. Bring one provider, MCP server, or workflow tool question and we can run a 20-minute preflight. I want your candid verdict on whether it helped.

### 7. markpalfreyman

> Hi [name], I saw your LangGraph report on checkpoint loss after a non-graceful exit. Nanmesh aims to put known integration failure modes in front of an agent before it makes a recommendation. Could you test it against one persistence or workflow choice you are making now? I can help connect it in 20 minutes.

### 8. MarioAlessandroNapoli

> Hi [name], your report on long tool calls re-running from checkpoints is a strong example of the decision context teams miss during tool selection. I am testing Nanmesh with builders who have one live integration choice. Bring a current agent, checkpoint, or provider decision and we can see whether the preflight earns its place in 20 minutes.

### 9. Correctover

> Hi [name], I read your LangGraph governance proposal for checkpointers. Nanmesh gives agents a preflight record before they recommend an integration, including known constraints and failure modes. Would you try it against one governance or reliability decision? I can work through the real case with you in 20 minutes.

### 10. makroumi

> Hi [name], your measurement of LangGraph checkpoint storage and token overhead is the kind of evidence I want an agent to see before it recommends a dependency. I am testing Nanmesh with people making a live tool choice. Could you bring one database, checkpoint, or agent platform decision to a 20-minute session?

### 11. AndrewOYLK

> Hi [name], I saw your ToolNode report on MCP content blocks. I am testing a preflight layer for agent integration decisions. It gives the agent a compact record of known constraints before it selects a tool. Bring one MCP compatibility question and we can test it in 20 minutes. I am looking for a useful failure case as much as a success.

### 12. Rul1an

> Hi [name], your work on carrying tool-call IDs through human approval interrupts maps to a real agent integration problem. I am testing Nanmesh against live tool-selection decisions. Would you bring one approval, MCP, or workflow integration question to a 20-minute session? I can help wire it into the smallest useful test.

### 13. edgarfloresguerra2011-a11y

> Hi [name], I read your proposal for MCP server security certification in CrewAI. I am testing Nanmesh, a preflight layer that helps an agent inspect an integration before it recommends it. Could you test it against one MCP server or tool trust decision? I can join for 20 minutes and keep the session focused on the decision in front of you.

### 14. SemeAIPletinnya

> Hi [name], your release-control mediation proposal for CrewAI is close to the problem I am testing. Nanmesh gives an agent evidence before it recommends or installs a tool. Bring one current execution-control, MCP, or provider decision and we can see whether the preflight catches anything useful in 20 minutes.

### 15. cschanhniem

> Hi [name], I saw your work on deterministic tool permission gating for CrewAI. I am testing Nanmesh with builders who need to choose tools under real constraints. Could you bring one tool authorization or integration decision to a 20-minute session? I will use your feedback to improve the evidence packet, not to ask for promotion.

### 16. abhinaykrupa

> Hi [name], I read your request for production safety guidance around local code execution in CrewAI. Nanmesh checks an integration record before an agent recommends a tool. Would you test it on one current code-execution, sandbox, or provider decision? I can help connect it around your actual case in 20 minutes.

### 17. azender1

> Hi [name], your report on duplicate payments, emails, and trades after tool retries shows why teams need better preflight evidence. I am testing Nanmesh with builders who have a current integration choice. Bring one side-effecting tool or workflow decision and we can test the layer in 20 minutes.

### 18. ritsth

> Hi [name], I saw your CrewAI report on tool calls disappearing in a recovery path. I am testing Nanmesh against real tool and provider decisions, with known failure modes available before an agent makes a recommendation. Could you bring one current reliability question to a 20-minute test?

### 19. nagasatish007

> Hi [name], your request for a tool authorization middleware hook is a good fit for the preflight work I am testing. Nanmesh gives agents evidence about an integration before they select it. Would you try it against one CrewAI tool, MCP server, or policy choice? I can help wire a focused 20-minute test.

### 20. maxpetrusenkoagent

> Hi [name], I read your production code-execution guide for CrewAI. I am testing Nanmesh with builders deciding between tools, sandboxes, providers, and MCP servers. Could you bring one current integration choice to a 20-minute session? I want to see whether the preflight helps someone who already knows the hard parts.

### 21. robo-trh

> Hi [name], your Mastra report on session reconnect and auth refresh points to a failure mode agents should see before they recommend an integration. I am testing Nanmesh with builders who have a live tool decision. Bring one session, streaming, or MCP question and we can test it in 20 minutes.

### 22. gregorskii

> Hi [name], I saw your request for workflow retry classification in Mastra. Nanmesh gives an agent a compact preflight record before it recommends a dependency. Would you test it against one workflow, provider, or tool decision you are making now? I can help set up the 20-minute test.

### 23. sachinp9797

> Hi [name], your MCP elicitation capability report is a useful example of the details that break integrations after a team has already chosen a tool. I am testing Nanmesh with people facing a live MCP or provider decision. Could you bring one current question to a 20-minute session? I want direct feedback on whether the evidence changes the choice.

### 24. roaminro

> Hi [name], I read your Mastra work on MCP notifications for tool, resource, log, and progress updates. I am testing Nanmesh, a preflight layer for agent integration choices. Would you try it against one current MCP server or tool decision? I can work through the real case with you in 20 minutes.

### 25. mstysk

> Hi [name], your request to pass Hono auth context into streamable HTTP MCP deployments is a good fit for this test. Nanmesh gives an agent a checked integration record before it recommends a tool. Bring one auth, MCP, or deployment decision and we can test whether the preflight helps in 20 minutes.

## Send Order

Send the first five through their listed public routes. Wait for replies before
sending the next five. Track the source link, contact date, reply, test date,
decision tested, and whether Nanmesh changed the decision. Do not count a
polite reply as a completed test.
