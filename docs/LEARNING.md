# Playing a mod the tools have never seen

HOI4 Live Tools has a typed tool for every part of the plain game: research, focuses, production,
construction, armies and so on. A total-conversion mod brings its own resources, buildings, panels and
mechanics that no tool knows. Your agent can learn them: find out what is there, try it, see what it did,
and keep what worked as a skill it can run again in later sessions and in campaign plans.

This works with any mod you have installed and enabled in the launcher. Nothing in the tools names a mod.

## 1. What is new in the mod

Before playing, read what the mod adds against the plain game. From the folder of HOI4 Live Tools:

```sh
python3 scripts/hoi4facts/facts.py mod-diff
```

It uses the mods enabled in the launcher (add `--mod PATH` for another `.mod` file or mod folder, and
`--mods none` to leave the enabled ones out; `--json` gives the full report). It lists:

- new and removed resources, buildings, idea and law categories, ideologies, decision categories and
  decisions (missions counted), battalion types, equipment, technologies, focus trees, army abilities,
  occupation laws, scripted effects and triggers;
- the mod's **scripted interfaces** (its own panels): which window each one shows, its buttons, and for each
  button whether a click spends something (political power, manpower, equipment, experience, stability) or
  cannot be undone (a declaration, an annexation, an idea added or removed);
- whether the map is replaced, which defines the mod changes, which interface files it replaces, and the
  variables its texts show;
- **typed tools likely affected**: tools whose buttons the mod's interface no longer defines. Use the steps
  below for those panels;
- **learn first**: the targets worth learning, most useful first.

A campaign planner (the strategist skill) runs this report before it writes a plan for a modded game.

## 2. Look at a panel

`hoi4_ui_inspect` reads every open window from the game's memory: each widget with a stable id (the names
the `.gui` files give, for example `topbar/decisionview_button`), its kind, its place on screen, whether the
game greys it out, the text of text boxes, and what the game's and the mod's files say about it: the
tooltip text, and for a mod's button the effect its click runs, what it spends and whether it can be undone.

```json
{"window": "topbar"}
```

Pass a window id, or a widget id to read only part of a panel. It is a pure read: no input, never pauses.

## 3. Try one thing

`hoi4_ui_act` clicks (or right-clicks, shift-clicks, hovers, scrolls) one widget by its id and answers what
changed: windows that opened or closed, widgets that appeared, changed text or were enabled or disabled,
and the country's numbers that moved (political power, stability, war support, manpower, factories,
resources, alerts).

```json
{"widget": "topbar/decisionview_button", "action": "click"}
```

It refuses before any input when the widget is not there, hidden, greyed out or has no known place on
screen. A click whose effect spends something or cannot be undone needs `"confirm_irreversible": true`, and
a button in a pop-up needs `"answer_dialog": true`, so the agent never does either by accident.

## 4. Keep what worked

`hoi4_skill_save` keeps a procedure as a skill: a name, its purpose, its parameters, the windows and widgets
that must be on screen first, its steps (tool calls), how to tell it worked, its cost and whether it can be
undone. `hoi4_fact_save` keeps a plain fact with its source.

```json
{"skill": {
  "name": "open_caps_trade",
  "purpose": "Open the mod's caps trade panel",
  "steps": [{"tool": "hoi4_ui_act", "args": {"widget": "topbar/caps_button"}}],
  "verify": [{"check": "window_opens", "id": "caps_window"}],
  "cost": "nothing",
  "reversible": true
}}
```

Steps may also be any typed tool. A string `"{amount}"` in a step is replaced by the run's argument
`amount`, which the skill names under `parameters`. Proofs (`verify`) are `window_opens`, `window_closes`,
`widget_changes` (each with an `id`) or `read_changes` (a read tool, the dotted path of a number in its
answer, and `up`, `down` or `any`).

`hoi4_skill_run` runs a saved skill: `{"name": "open_caps_trade"}`. `hoi4_skill_list` and `hoi4_skill_show`
show what is saved; `hoi4_skill_retire` takes a skill out of use.

## Where skills live, and sharing them

Skills and facts are plain JSON files you can read and edit:

```
<install folder>/learned/skills/<name>.json
<install folder>/learned/facts.json
```

(`HOI4_LIVE_LEARNED` points somewhere else.) To share a skill, send the file; the receiver drops it into
the same folder. Each skill records the game version and the mod set it was learned on. On another game
version or mod set it runs only with `"revalidate": true`; a successful run moves it to the current one.

## When a skill breaks

A mod update or a game patch can change a panel. A skill whose required windows or widgets are missing
when it is run is marked **stale** and refused from then on, with the reason in its file. Fix its steps and
save it again (`"replace": true`), or retire it.

## Skills in a campaign plan

A plan can run learned skills on its own with the `learned` key:

```json
"learned": [{"skill": "buy_water_caps", "args": {"amount": 2}, "when": "at_war_with:NCR",
             "every_days": 14, "max": 10}]
```

The executor runs each listed skill with `hoi4_skill_run` when its condition holds, at most `max` times and
at most once every `every_days`. A skill that cannot be undone runs only with `"confirm_irreversible": true`
in its row. Review a skill with `hoi4_skill_show` before you list it.

## What the agent may and may not do

- It acts only through the game's own interface, as a player would: no console, no debug commands, no save
  editing, no changes to the game's memory, the same input lease as every other action.
- It clicks only widgets that are shown; greyed-out widgets, the console and any debug control (even a
  mod's own debug button) are refused.
- It never spends something or does something final without being told to (`confirm_irreversible`), and
  never answers a pop-up without `answer_dialog`.
- In a campaign it never reloads to undo a try.

## Limits today

- Window reading is built for the macOS runtime.
- Tooltip text comes from the interface files; the game's live tooltip (with numbers filled in) is not read.
- Places on screen are known for buttons, icons and check boxes; other widgets are read but cannot be
  clicked directly (click a button inside them).
- A mod window that the game keeps outside its usual window lists may not be listed yet.
