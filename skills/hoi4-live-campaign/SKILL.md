---
name: hoi4-live-campaign
description: Route HOI4 campaign play to the current FAIR/META campaign playbook, with live telemetry, owned input, no cheats and recorded acceptance requirements.
---

# HOI4 campaign play

Read [the current campaign playbook](../hoi4-campaign-playbook/SKILL.md) before
starting a campaign. It supersedes this skill's historical save-and-poll loop,
test-mode setup, two-day pause chunks and skipped-failure queues.

Campaigns start in FAIR mode by default. Test mode is a separate QA session and
must never prepare or rescue a showcase. Germany and Nationalist China use FAIR;
the third showcase, Poland, uses META. Both modes prohibit console/debug/eval,
forced effects, save editing, memory writes, hidden intelligence and undo reloads.

Check [the Mac continuation status](../../docs/MAC_CONTINUATION_STATUS.md) for
proven capabilities and remaining implementation gaps. A declared plan or an
offline test is not evidence that its actions work in the live game. Do not start
acceptance until the prerequisites and supported action adapters are ready.
