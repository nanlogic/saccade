# Saccade AI-041 Current-Tab Dogfood: SimpleMMO

Date: 2026-07-16
Site: `https://web.simple-mmo.com/`
Starting and final page: `https://web.simple-mmo.com/events`
Verdict: ready for LLM-directed gameplay dogfood after the user enables the tab;
the current Saccade contract still owns too much action policy

## Scope and safety

The user authorized a bounded exploration of the current Agent On tab. The run
allowed redacted reading, same-origin navigation and reversible filter clicks.
It explicitly excluded registration, login changes, messages/chat, rewards,
starter packs, battle, purchases, market actions and submissions.

No cookie, browser storage, protected value or bridge capability was returned.
No account or game-state action was attempted.

## What the website is

SimpleMMO is a browser-based, text/menu-driven multiplayer RPG. The surfaces
visible through the current session include Home, Town, Inventory, Battle,
Quests, Character, Profession, Crafting, Tasks, Collections, Guilds, Travel,
messages, notifications and chat. It also exposes a starter pack, daily reward,
premium diamonds, a diamond market/store and a World Cup 2026 collection event.

The current session appears to be in a guest or incomplete-registration state:
the page warns that progress may be lost and offers `Register Account`. The
events page currently contains two System notifications saying avatars were
added to inventory for the first and second community milestones.

## What Saccade successfully did

1. Attached to the user-granted current tab without receiving cookies, storage
   or raw browser capabilities.
2. Identified the starting page as `Notification | SimpleMMO` at `/events`.
3. Navigated to the same-origin home page and read a 1,569-character bounded
   article packet with headings and source/revision binding.
4. Inventoried 28 exposed home-page actions, including navigation to Tasks,
   Collections, Guilds, Travel, Inventory, Battle, Quests, Character,
   Profession and Crafting.
5. Restored `/events` and read a 1,054-character bounded event packet plus more
   than 40 exposed controls.
6. Dispatched the non-destructive `System` filter through the same WebView. The
   verified result changed the URL to `/events?type=SYSTEM` at revision 47.
7. Dispatched `All` and restored `/events` at revision 48.

The tab was left on its original page and filter. The Agent permission was not
changed.

## What an LLM can do under one gameplay grant

- Summarize notifications, game updates and public/help content.
- Explain the visible systems and compare Tasks, Collections, Guilds, Travel,
  Inventory, Crafting and event surfaces.
- Navigate to explicitly requested, low-risk read-only pages.
- Filter notification categories and report the verified result.
- Play battles and quests, travel, manage normal inventory and choose routine
  progression actions within the user's task.
- Claim free rewards and open free packs when the gameplay grant allows them.
- Review account, inventory and character state while choosing the next move.

## One-confirmation LLM workflow

The user should approve the session once, in plain language. For example:

> Play SimpleMMO for 20 minutes. Claim free rewards, do quests, battle and
> travel. Do not spend diamonds or money, trade/delete assets, or message
> anyone.

The LLM host interprets that mandate and decides how to play. Saccade does not
interpret the time limit, spending rule or communication rule. It enforces the
tab's Human-controlled Agent On/Off state, keeps protected values outside model
context, binds each click to the current page revision and records a receipt.

The user and LLM host may use any task policy, including permission to spend or
communicate. Saccade does not add its own approval rule for those actions.

## Saccade product findings

### Passed

- Current-tab Agent On attachment.
- URL/title/revision-bound redacted article extraction.
- Same-WebView navigation.
- Revision-bound safe action execution.
- Post-action verification and reversible restoration.
- Negative privacy boundary: no cookies, storage, protected values or
  capability tokens appeared.

### Defects or gaps

1. **Saccade owns too much action policy.** The current browser classifies and
   blocks submit, payment, publication and other site actions. The LLM host
   should own those decisions. Saccade should return action facts, execute
   revision-bound input on an Agent On tab and protect model-invisible values.
2. **MCP navigation and adapter capabilities disagree.** The MCP schema accepts
   `back`, but this CEF adapter did not advertise it. Restoration required an
   explicit navigation to the original URL.
3. **The action inventory contains duplicate logical controls.** Several items
   appear as both a button and a link, including All, System, messages and
   notifications. Agents need one canonical action per logical target.
4. **Collector readiness was initially transient.** The first `/events`
   article request returned `renderer collector is not ready`; extraction
   became healthy after navigation and remained healthy through revisions
   42–48.

## Game/product review

SimpleMMO is a strong Saccade dogfood target because it combines dense
navigation, dynamic account state, social surfaces, low-risk filters and
high-risk persistent actions in one site. It tests whether the Agent can be
useful without quietly turning “I can see a button” into “I may click it.”

From a short-loop game-design perspective, the observed home page sells many
meta systems such as daily rewards, starter packs, premium currency, tasks,
collections, guilds and events. We did not exercise the first 10–60 second play
loop.
The page promises an immediate start and reward, yet does not expose enough
read-only evidence to establish the key loop: what action gives immediate
feedback, what becomes almost reachable, what relationship reverses, or what
failure teaches the next attempt. A real gameplay comparison should begin only
after the user explicitly authorizes a disposable or low-stakes play session.

## Overall assessment

For this site, AI-041 is already useful as a privacy-bounded reader and
navigation copilot. The System-filter test demonstrates a real closed loop:
discover action → bind to revision → execute → verify URL/content → restore.

AI-041 has the input and verification machinery for autonomous gameplay. The
next product gate removes Saccade-owned site-action policy, keeps the protected
data boundary and fixes canonical action mapping plus browser navigation.
