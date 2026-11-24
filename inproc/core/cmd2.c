/* HOI4 Live Tools in-process module: stage 2 scans (the generic command builder). See cmd2.h. Pure computation. */
#include "cmd2.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The command families, in order of time saved (BATCHING-AUDIT-1123). Layouts read from 1.19.4.1 and identical on
 * 1.20.0.0 (INPROCESS-COMMANDS-1123: 422/422 command classes unchanged); every entry is re-proven on the running build
 * by scan_cmd_type, and the protocol field ids are part of the proof (they are the network protocol, stable by design). */
const cmd_spec CMD2_SPECS[] = {
    /* production: the +/- factory buttons of a line (UI ctor: (line, delta)); one command carries any delta */
    {"production", "production.add_factories", "CAddProductionLineFactoriesCommand", 0x30, BUILD_FIELDS, CTOR_NONE, 0,
     0x24, "24CEquipmentProductionLine", 2,
     {{"line", 0x24, FK_REF, 0, 0x2fb3}, {"delta", 0x2c, FK_I32, 0, 0x2f76}}, OBJ2_NONE, 0},
    /* production: a line's close button */
    {"production", "production.remove_line", "CRemoveProductionLineCommand", 0x30, BUILD_FIELDS, CTOR_NONE, 0, 0x24,
     "24CEquipmentProductionLine", 1, {{"line", 0x24, FK_REF, 0, 0x2fb3}}, OBJ2_NONE, 0},
    /* air: rebase. The UI's rebase helper builds, per selected wing not already at the base, Transfer(wing, base, 0, NULL, 0) */
    {"air", "air.transfer", "CStratAirTransferCommand", 0x40, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "8CAirWing", 2,
     {{"wing", 0x24, FK_REF, 0, 0x2fb5}, {"base", 0x2c, FK_REF, 0, 0x2fb6}}, OBJ2_AIR_BASE, 0},
    /* air: a mission bit on a wing (the mission toolbar; the UI posts one per selected wing: (wing, 1<<mission, enable)) */
    {"air", "air.set_mission", "CStratAirSetMissionCommand", 0x48, BUILD_CTOR, CTOR_WING_I32_U8, 1, 0x28, "8CAirWing", 3,
     {{"wings", 0x28, FK_REF_ARRAY, 0x34, 0x3ba6}, {"mission_mask", 0x40, FK_I32, 0, 0xe1}, {"enable", 0x44, FK_U8, 0, 0x2888}}, OBJ2_NONE, 0},
    /* air: a wing to a strategic region (the region right click; ctor (wing, region id, NULL)) */
    {"air", "air.enable_mission", "CStratAirEnableMissionCommand", 0x60, BUILD_CTOR, CTOR_WING_I32_NULL, 1, 0x28,
     "8CAirWing", 2, {{"wings", 0x28, FK_REF_ARRAY, 0x34, 0x2fb5}, {"region_id", 0x58, FK_I32, 0, 0x2eee}}, OBJ2_REGION, 0},
    /* ---- engineer 71 (24 Nov): the rest of the air family (static from 1.19.4.1 and 1.20.0.0; payloads identical) ----
     * air: split. The UI's split helper (1.20 0x100044e70) posts, per selected wing, Split(wing, aircraft / 2) with no
     * "can do" call of its own; the module calls the command's check anyway (it asks the wing whether it can split n). */
    {"airwing", "airwing.split", "CStratAirSplitCommand", 0x30, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "8CAirWing", 2,
     {{"wing", 0x24, FK_REF, 0, 0x2fb5}, {"aircraft", 0x2c, FK_I32, 0, 0x2f8e}}, OBJ2_NONE, 0},
    /* air: merge. One command carries the whole group (the Merge button: Consolidate(CPdxArray<CAirWing *>)) */
    {"airwing", "airwing.merge", "CStratAirConsolidateCommand", 0x40, BUILD_CTOR, CTOR_ARRAY, 1, 0x28, "8CAirWing", 1,
     {{"wings", 0x28, FK_REF_ARRAY, 0x34, 0x2fb5}}, OBJ2_NONE, 1},
    /* air: disband (the Air Forces Overview's Disband: Delete(&owner tag, wing); the equipment goes to the reserve) */
    {"airwing", "airwing.disband", "CDeleteAirWingCommand", 0x48, BUILD_CTOR, CTOR_TAGP_OBJ, 1, 0x28, "8CAirWing", 2,
     {{"wings", 0x28, FK_REF_ARRAY, 0x34, 0x3369}, {"tag", 0x40, FK_I32, 0, 0x289a}}, OBJ2_NONE, 0},
    /* air: attach to an army (right click on the army card: Attach(wing, army)); the army is a COrdersGroup */
    {"airwing", "airwing.attach", "CAttachAirWingToArmyCommand", 0x48, BUILD_CTOR, CTOR_OBJ_OBJ2, 1, 0x28, "8CAirWing", 2,
     {{"wings", 0x28, FK_REF_ARRAY, 0x34, 0x3369}, {"army", 0x40, FK_REF, 0, 0x289d}}, OBJ2_ORDERS_GROUP, 0},
    /* airwing: a mission region by id: a region with no own base (the map's region right click needs no base: the
     * command carries the region id; the game's own check reads the region from the game state's region array and
     * judges range), and 0 = HOLD: the Hold button's helper (1.20 0x100042fe0) posts EnableMission(wing, 0, NULL)
     * (after a CancelTransfer when the wing is flying a transfer: such wings stay on the GUI path) */
    {"airwing", "airwing.region", "CStratAirEnableMissionCommand", 0x60, BUILD_CTOR, CTOR_WING_I32_NULL, 1, 0x28,
     "8CAirWing", 2, {{"wings", 0x28, FK_REF_ARRAY, 0x34, 0x2fb5}, {"region_id", 0x58, FK_I32, 0, 0x2eee}}, OBJ2_REGION_ID, 0},
    /* ---- production2 (engineer 71): a NEW line and the priority order; one live pass for both ----
     * add_line: the production view's Add (UI helper 1.20 0x10211e320): AddProductionLine(&own tag, variant, organisation
     * or NULL, at_top). Fields: tag +0x24 (0x6b), variant handle +0x28 (0x2f4e), organisation handle +0x30 (0x4ad8,
     * written only when set), at_top byte +0x38 (0x25a; execution then calls the same set-priority-0 routine as the
     * priority command). Authorisation: the sender controls the tag; the "can do" check casts the variant to
     * CEquipmentVariant and asks whether the country may produce it. The organisation stays NULL until the module
     * can name an organisation object (MIO objects are not read yet). */
    {"production2", "production2.add_line", "CAddProductionLineCommand", 0x40, BUILD_CTOR, CTOR_ADDLINE, 0, 0x24,
     "17CEquipmentVariant", 2, {{"tag", 0x24, FK_I32, 0, 0x6b, 0}, {"variant", 0x28, FK_REF, 0, 0x2f4e, 0}}, OBJ2_NONE, 0,
     AUTH_TAG, "17CEquipmentVariant"},
    /* priority: the line's arrows / Shift+click: ChangePriority(line, index) with the absolute place (callers pass 0 for
     * the top and count-1 for the bottom); the factory's default index is -1 */
    {"production2", "production2.priority", "CChangeProductionLinePriorityCommand", 0x30, BUILD_FIELDS, CTOR_NONE, 0, 0x24,
     "15CProductionLine", /* the base class: equipment AND naval lines (its resolver casts to CProductionLine) */ 2, {{"line", 0x24, FK_REF, 0, 0x2fb3, 0}, {"index", 0x2c, FK_I32, 0, 0x2f72, -1}},
     OBJ2_NONE, 0},
    /* air: detach from its army (Detach(wing)) */
    {"airwing", "airwing.detach", "CDetachAirWingFromArmyCommand", 0x40, BUILD_CTOR, CTOR_OBJ, 1, 0x28, "8CAirWing", 1,
     {{"wings", 0x28, FK_REF_ARRAY, 0x34, 0x3369}}, OBJ2_NONE, 0},
    /* ---- orders (engineer 73, 24 Nov): battle plans. One object = the orders group (army or army group) whose plan
     * the command edits; the order is named by its id, found with the game's own lookup (the one every order
     * command's "can do" calls). Layouts read from 1.20.0.0 and 1.19.4.1 (identical field ids); see
     * INPROCESS-COMMANDS-1123.md "Battle plans BUILT".
     * delete ONE order (the plan bar's delete on one order): Delete(group, id); the check: lookup(group, id) != NULL */
    {"orders", "orders.delete", "COrderDeleteCommand", 0x30, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "12COrdersGroup", 2,
     {{"group", 0x24, FK_REF, 0, 0x3f, 0}, {"order_id", 0x2c, FK_I32, 0, 0x3036, 0}}, OBJ2_NONE, 0},
    /* execute / stop: Execute(group, id or 0 = the whole plan, execute); the factory's defaults are execute 1, +0x31 0
     * (+0x31, id 0x2b97, the plan bar's Shift-toggle path, is left at its default) */
    {"orders", "orders.execute", "COrderExecuteCommand", 0x38, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "12COrdersGroup", 3,
     {{"group", 0x24, FK_REF, 0, 0x3f, 0}, {"order_id", 0x2c, FK_I32, 0, 0x3036, 0}, {"execute", 0x30, FK_U8, 0, 0x3041, 1}},
     OBJ2_NONE, 0},
    /* a new province path for an EXISTING order (the front tool's edit of a drawn line, the arrow's path): the UI's
     * constructor (cmd, order, &provinces, 1); the check: the group is a COrdersGroup, lookup(group, id) != NULL and
     * the order's lock handle is empty. The order keeps its id (and what hangs on it) */
    {"orders", "orders.set_path", "COrderSetPathCommand", 0x50, BUILD_CTOR, CTOR_ORDER_PATH, 0, 0x24, "12COrdersGroup", 3,
     {{"provinces", 0x30, FK_I32_ARRAY, 0x3c, 0x174, 0}, {"group", 0x24, FK_REF, 0, 0x3f, 0}, {"order_id", 0x2c, FK_I32, 0, 0x3036, 0}},
     OBJ2_NONE, 0},
    /* a new front line under a parent order (the front tool's commit): the UI's constructor with the divisions the UI
     * hands over (the parent's divisions no child order holds), flags 0 (not a spearhead, not a withdraw), the drawing
     * group = obj2 or the group itself. The check: provinces given, groups live, parent id set, lookup(group, parent);
     * execution keeps only provinces the country controls, split into runs of consecutive valid ids */
    {"orders", "orders.new_front", "COrderNewFrontCommand", 0x70, BUILD_CTOR, CTOR_NEW_FRONT, 0, 0x24, "12COrdersGroup", 4,
     {{"provinces", 0x50, FK_I32_ARRAY, 0x5c, 0x174, 0}, {"group", 0x24, FK_REF, 0, 0x3f, 0}, {"parent_id", 0x68, FK_I32, 0, 0x152, 0},
      {"divisions", 0x38, FK_REF_ARRAY, 0x44, 0x28a3, 0}}, OBJ2_NONE, 0},
    /* ---- orders2 (engineer 74, 24 Nov): the rest of the battle plan, one live pass (Session S5-PLAN). Layouts read from
     * 1.20.0.0 and 1.19.4.1 (identical field ids and offsets); every scalar takes the request's vals by its 1-based index
     * (vi); arrays are filled through the fresh object's own allocator (BUILD_FIELDS, see cmd2.h).
     * new_root: the front tool's click on a border (UI 1.20 0x1025bb2d0): NewRoot(group, &front handle, section id, from,
     * to, the group's divisions, split). The game binds the root to that CFront section; its own check: the group is a
     * live COrdersGroup and the front handle is not empty. split = the army group's "divide among the armies" (the UI
     * passes 1 for an army group unless Shift is held, 0 for an army): Shift+click = 0 = ONE field-marshal line */
    {"orders2", "orders2.new_root", "COrderNewRootCommand", 0x70, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "12COrdersGroup", 7,
     {{"group", 0x24, FK_REF, 0, 0x3f, 0}, {"divisions", 0x30, FK_REF_ARRAY, 0x3c, 0x28a3, 0}, {"front", 0x48, FK_HANDLE, 0, 0x29e0, 0},
      {"section", 0x50, FK_I32, 0, 0x3325, 0, 1}, {"from", 0x58, FK_I64, 0, 0x298f, 0, 2}, {"to", 0x60, FK_I64, 0, 0x2990, 100000, 3},
      {"split", 0x6c, FK_U8, 0, 0x4c12, 1, 4}}, OBJ2_FRONT, 0},
    /* assign divisions to an order (the plan bar's assign: Assign(&divisions, group, order id)); the check resolves every
     * division as a CUnit whose army is not locked, and the group as a COrdersGroup */
    {"orders2", "orders2.assign", "COrderAssignCommand", 0x50, BUILD_FIELDS, CTOR_NONE, 0, 0x40, "12COrdersGroup", 3,
     {{"group", 0x40, FK_REF, 0, 0x3f, 0}, {"divisions", 0x28, FK_REF_ARRAY, 0x34, 0x28a3, 0}, {"order_id", 0x48, FK_I32, 0, 0x3036, 0, 1}},
     OBJ2_NONE, 0},
    /* remove divisions from the group's orders (the plan bar's unassign); the two flags keep the factory's values */
    {"orders2", "orders2.unassign", "COrderUnassignCommand", 0x50, BUILD_FIELDS, CTOR_NONE, 0, 0x40, "12COrdersGroup", 2,
     {{"group", 0x40, FK_REF, 0, 0x3f, 0}, {"divisions", 0x28, FK_REF_ARRAY, 0x34, 0x28a3, 0}}, OBJ2_NONE, 0},
    /* a fallback line by province list (the fallback tool: NewFallback(group, &divisions, &provinces)); the check: the
     * group is a COrdersGroup and the list is not empty */
    {"orders2", "orders2.fallback", "COrderNewFallbackCommand", 0x60, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "12COrdersGroup", 3,
     {{"group", 0x24, FK_REF, 0, 0x3f, 0}, {"divisions", 0x30, FK_REF_ARRAY, 0x3c, 0x28a3, 0}, {"provinces", 0x48, FK_I32_ARRAY, 0x54, 0x174, 0}},
     OBJ2_NONE, 0},
    /* a root's stretch of its front section (EditRoot(group, root id, from, to); the refresh byte keeps its default 1) */
    {"orders2", "orders2.edit_root", "COrderEditRootCommand", 0x48, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "12COrdersGroup", 4,
     {{"group", 0x24, FK_REF, 0, 0x3f, 0}, {"order_id", 0x2c, FK_I32, 0, 0x3036, 0, 1}, {"from", 0x30, FK_I64, 0, 0x298f, 0, 2},
      {"to", 0x38, FK_I64, 0, 0x2990, 100000, 3}}, OBJ2_NONE, 0},
    /* the landing province of an existing naval-invasion order (the invasion tool's target click); the check: the order
     * exists in the group */
    {"orders2", "orders2.invasion_target", "CAddNavalInvasionTargetCommand", 0x38, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "12COrdersGroup", 3,
     {{"group", 0x24, FK_REF, 0, 0x3f, 0}, {"order_id", 0x2c, FK_I32, 0, 0x3036, 0, 1}, {"province", 0x30, FK_I32, 0, 0x2840, 0, 2}},
     OBJ2_NONE, 0},
    /* the drop province of an existing paradrop order */
    {"orders2", "orders2.paradrop_target", "COrderSetParadropTargetCommand", 0x38, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "12COrdersGroup", 3,
     {{"group", 0x24, FK_REF, 0, 0x3f, 0}, {"order_id", 0x2c, FK_I32, 0, 0x3036, 0, 1}, {"province", 0x30, FK_I32, 0, 0x2840, 0, 2}},
     OBJ2_NONE, 0},
    /* a SPEARHEAD under a parent order: the front tool in its blitz mode (UI mode 3) posts NewFront with the blitz byte
     * set; everything else as orders.new_front */
    {"orders2", "orders2.spearhead", "COrderNewFrontCommand", 0x70, BUILD_CTOR, CTOR_NEW_FRONT, 0, 0x24, "12COrdersGroup", 4,
     {{"provinces", 0x50, FK_I32_ARRAY, 0x5c, 0x174, 0}, {"group", 0x24, FK_REF, 0, 0x3f, 0}, {"parent_id", 0x68, FK_I32, 0, 0x152, 0},
      {"divisions", 0x38, FK_REF_ARRAY, 0x44, 0x28a3, 0}}, OBJ2_NONE, 0},
    /* ---- units (engineer 75, 24 Nov): ARMY STRUCTURE, one live pass (Session S6-UNITS). Read from 1.20.0.0 and 1.19.4.1
     * (identical field ids and offsets). Which commands carry membership: COrderGroupCommand (a NEW army of the given
     * divisions: the new-army card), COrderAssignCommand (divisions INTO an existing army: the right click on its card
     * posts Assign(divisions, that army, the order the UI picks or 0)), CArmyGroupCommand / CAssignToArmyGroupCommand /
     * CRemoveFromArmyGroupCommand (armies into a new / an existing army group, out of one). CMoveCommand is the map
     * MOVE order (a province target), not membership; CMergeArmiesCommand merges DIVISIONS (CArmy is the division class;
     * "cannot merge less than 2 armies"), not armies.
     * new_army: the new-army card (UI 1.20 0x100d28480): COrderGroupCommand(&divisions, the first division's theatre, NULL).
     * The constructor keeps only divisions the theatre's country owns and that may change army (the game's own filter);
     * the check: the theatre resolves, every division resolves as a CUnit, the theatre's army count is under its cap */
    {"units", "units.new_army", "COrderGroupCommand", 0x58, BUILD_CTOR, CTOR_DIVS_THEATRE, 0, 0x40, "8CTheatre", 2,
     {{"divisions", 0x28, FK_REF_ARRAY, 0x34, 0x28a3, 0}, {"theatre", 0x40, FK_REF, 0, 0x2f1a, 0}}, OBJ2_NONE, 1},
    /* transfer: divisions INTO an existing army (the right click on its card): Assign(&divisions, army, order id). The
     * order is the one the UI picks: the first top order of the army (or of the nearest parent group that has one),
     * descended through first children unless its kind is 3; 0 when there is none. Assign's check does not read the id */
    {"units", "units.transfer", "COrderAssignCommand", 0x50, BUILD_FIELDS, CTOR_NONE, 0, 0x40, "12COrdersGroup", 3,
     {{"group", 0x40, FK_REF, 0, 0x3f, 0}, {"divisions", 0x28, FK_REF_ARRAY, 0x34, 0x28a3, 0}, {"order_id", 0x48, FK_I32, 0, 0x3036, 0, 1}},
     OBJ2_NONE, 0},
    /* leader: the commander list's pick: SetArmyLeader(leader or NULL, group, 0) (every UI caller passes 0); NULL = unassign */
    {"units", "units.leader", "CSetArmyLeaderCommand", 0x38, BUILD_CTOR, CTOR_LEADER, 0, 0x2c, "12COrdersGroup", 3,
     {{"leader", 0x24, FK_REF, 0, 0x28b7, 0}, {"group", 0x2c, FK_REF, 0, 0x3f, 0}, {"flag", 0x34, FK_U8, 0, 0x282b, 0, 0, 1}},
     OBJ2_CAST, 0, AUTH_HANDLE, NULL, "11CArmyLeader", 1},
    /* rename: the badge's name box: SetOrderGroupName(group, &name) (a libc++ std::string copied into +0x30) */
    {"units", "units.rename", "CSetOrderGroupNameCommand", 0x48, BUILD_CTOR, CTOR_GROUP_NAME, 0, 0x24, "12COrdersGroup", 2,
     {{"group", 0x24, FK_REF, 0, 0x3f, 0}, {"name", 0x30, FK_STR, 0, 0x1b, 0}}, OBJ2_NONE, 0},
    /* a new army group of these armies (the new-army-group card): ArmyGroup(&armies, theatre) */
    {"units", "units.army_group_new", "CArmyGroupCommand", 0x80, BUILD_CTOR, CTOR_GROUPS_OBJ, 0, 0x24, "8CTheatre", 2,
     {{"armies", 0x30, FK_REF_ARRAY, 0x3c, 0x3f, 0}, {"theatre", 0x24, FK_REF, 0, 0x2f1a, 0}}, OBJ2_NONE, 1},
    /* armies into an existing army group: AssignToArmyGroup(&armies, army group, -1) (every UI caller passes -1) */
    {"units", "units.to_army_group", "CAssignToArmyGroupCommand", 0x50, BUILD_CTOR, CTOR_GROUPS_OBJ_I32, 1, 0x28, "12COrdersGroup", 3,
     {{"armies", 0x28, FK_REF_ARRAY, 0x34, 0x3f, 0}, {"army_group", 0x40, FK_REF, 0, 0x3892, 0}, {"index", 0x48, FK_I32, 0, 0x4c, -1}},
     OBJ2_CAST, 1, AUTH_HANDLE, NULL, "10CArmyGroup", 0},
    /* armies out of their army group: RemoveFromArmyGroup(&armies) */
    {"units", "units.from_army_group", "CRemoveFromArmyGroupCommand", 0x40, BUILD_CTOR, CTOR_GROUPS, 1, 0x28, "12COrdersGroup", 1,
     {{"armies", 0x28, FK_REF_ARRAY, 0x34, 0x3f, 0}}, OBJ2_NONE, 1},
    /* an army (or army group) shell removed: DeleteOrderGroup(group); the game's check decides whether it may go */
    {"units", "units.delete_group", "CDeleteOrderGroupCommand", 0x30, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "12COrdersGroup", 1,
     {{"group", 0x24, FK_REF, 0, 0x3f, 0}}, OBJ2_NONE, 0},
    /* ---- picks (engineer 75, 24 Nov): the player's own choices. Read from 1.20.0.0 and 1.19.4.1.
     * focus: the focus tree's start button posts SetNationalFocus(tag, focus): tag +0x24 (the sender must control it),
     * the focus by POINTER at +0x28 (written by its name, +0x18 of the focus). The "can do" check is the focus's own
     * virtual "can start" (vtable +0x50) with the tag's country: prerequisites, exclusivity, availability, not done. */
    {"picks", "picks.focus", "CSetNationalFocusCommand", 0x30, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "14CNationalFocus", 2,
     {{"tag", 0x24, FK_TAG, 0, 0x289a, 0}, {"focus", 0x28, FK_PTR, 0, 0x33b7, 0}}, OBJ2_NONE, 0,
     AUTH_TAG_PTR, "14CNationalFocus"},
    /* ---- events (engineer 76, 24 Nov): an event answer, as the event window's option click posts it (UI 1.20 0x101a91d60):
     * SelectEventOption(window's pending id, window's event, &window's country, &window's scope, the option's index).
     * Fields (1.20.0.0 and 1.19.4.1, identical): pending id +0x24 (0xb), event handle +0x28 (0x1b8), country +0x30
     * (0x292e; the sender must control it), scope +0x38..+0xe8 (0x2996, copied by the game's routine), option +0xe8
     * (0x2966). The "can do": the event resolves to a CEvent with options, the country is set, the game state holds a
     * pending record of the id, 0 <= option < the event's option count. The module adds the window's own gate (it posts
     * only while the window accepts a click) and the game's option test for the option (what the window shows). */
    {"events", "events.answer", "CSelectEventOptionCommand", 0xf0, BUILD_CTOR, CTOR_EVENT_OPTION, 0, 0x30, "6CEvent", 4,
     {{"id", 0x24, FK_I32, 0, 0xb, 0}, {"event", 0x28, FK_REF, 0, 0x1b8, 0}, {"country", 0x30, FK_I32, 0, 0x292e, 0},
      {"option", 0xe8, FK_I32, 0, 0x2966, 0}}, OBJ2_NONE, 0, AUTH_EVENT, "12CEventWindow"},
    /* ---- decisions (engineer 76): the decisions panel's select posts SelectDecision(tag, decision): the shape of
     * picks.focus (tag +0x24, the decision by RAW POINTER +0x28, written by the serializer). The "can do" 1.20 0x100216030:
     * a decision is named, the tag is set, the decision's own allowed/visible test with the tag's country, its active
     * state, its cost. Targeted decisions are another command (CSelectTargetedDecision, not read) */
    {"decisions", "decisions.take", "CSelectDecisionCommand", 0x30, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "9CDecision", 2,
     {{"tag", 0x24, FK_TAG, 0, 0x289a, 0}, {"decision", 0x28, FK_PTR, 0, 0x2b86, 0}}, OBJ2_NONE, 0,
     AUTH_TAG_PTR, "9CDecision"},
    /* ---- diplomacy (engineer 78, 24 Nov): the diplomacy view's actions. Every one is ONE CDiplomaticActionCommand (type
     * 10545, 0x30) holding a CDiplomaticAction by pointer at +0x28 (the network writes it by its own type). The UI builds
     * the action (sender = the player, receiver = the shown country, the date), hands it to the game's command maker (it
     * stamps the date and moves the action into the command) and posts. The command's "can do" (1.20 0x100f83de0): the
     * action is set, both tags > 0, the base check, then the ACTION's own legality virtual (+0x140), the same call the
     * view's Send button is enabled by. Auth: the sender controls the action's sender tag (+0x10).
     * Read from 1.20.0.0 and 1.19.4.1; the scan proves each class's constructor, size (its Clone), type id (SetType). */
    {"diplomacy", "diplomacy.guarantee", "CDiplomaticActionCommand", 0x30, BUILD_CTOR, CTOR_DIPLO_SIMPLE, 0, 0x28,
     "17CDiplomaticAction", 0, {{0}}, OBJ2_NONE, 0, AUTH_DIPLO, "16CGuaranteeAction"},
    {"diplomacy", "diplomacy.military_access", "CDiplomaticActionCommand", 0x30, BUILD_CTOR, CTOR_DIPLO_SIMPLE, 0, 0x28,
     "17CDiplomaticAction", 0, {{0}}, OBJ2_NONE, 0, AUTH_DIPLO, "21CMilitaryAccessAction"},
    {"diplomacy", "diplomacy.offer_military_access", "CDiplomaticActionCommand", 0x30, BUILD_CTOR, CTOR_DIPLO_SIMPLE, 0, 0x28,
     "17CDiplomaticAction", 0, {{0}}, OBJ2_NONE, 0, AUTH_DIPLO, "26COfferMilitaryAccessAction"},
    {"diplomacy", "diplomacy.non_aggression", "CDiplomaticActionCommand", 0x30, BUILD_CTOR, CTOR_DIPLO_SIMPLE, 0, 0x28,
     "17CDiplomaticAction", 0, {{0}}, OBJ2_NONE, 0, AUTH_DIPLO, "24CNonAggressionPactAction"},
    {"diplomacy", "diplomacy.improve_relations", "CDiplomaticActionCommand", 0x30, BUILD_CTOR, CTOR_DIPLO_SIMPLE, 0, 0x28,
     "17CDiplomaticAction", 0, {{0}}, OBJ2_NONE, 0, AUTH_DIPLO, "22CImproveRelationAction"},
    /* declare war: the declare-war dialog's selection handler writes the chosen war goal's handle into the action
     * (+0x6c) and its check box into the flag byte (+0x74, default 0); the goal is one of the player's own war goals
     * whose target is the receiver (the list the dialog shows) */
    {"diplomacy", "diplomacy.declare_war", "CDiplomaticActionCommand", 0x30, BUILD_CTOR, CTOR_DIPLO_WAR, 0, 0x28,
     "17CDiplomaticAction", 0, {{0}}, OBJ2_NONE, 0, AUTH_DIPLO, "17CDeclareWarAction"},
    /* ---- holds (engineer 79, 24 Nov): the AREA DEFENCE order, as the battle-plan map tool posts it. Read from 1.20.0.0
     * and 1.19.4.1 (identical layout and code). The area button only arms the tool; each left click on a state posts
     * EditAreaDefenseState(group, order id, the state's id, settings) through the tool's own constructor (1.20 0x10094a330,
     * UI 0x1025b7f80): order id 0 = no area order is being edited -> the execution creates a NEW area-defence order of the
     * group with that state; an order id = the state is TOGGLED in that order (added when absent, removed when present).
     * The settings mask is the tool's (0 on a fresh tool; an edited order's own mask). Fields: group +0x24 (0x3f), order
     * id +0x2c (0x3036), the state list +0x30/+0x3c (one int per click, 0x1b7), replace byte +0x48 (0x29f1, the
     * constructor writes 0: toggle), has-settings +0x49 (1), settings +0x4c (0x2b5d). The "can do": the group resolves
     * as a COrdersGroup (and an order named with the replace byte exists). CCreateAreaDefenseCommand (the order's own
     * create button path: an EMPTY order) is not used: the click is what places states. */
    /* ---- research (engineer 79, 24 Nov): a technology into a research slot, as the research view posts it. Read from 1.20.0.0
     * and 1.19.4.1. CSetResearchCommand 12144 (0x38): tag +0x24 (0x289a; the sender must control it), the technology
     * TEMPLATE by raw pointer +0x28 (0x285f, a CTechnologyTemplate of the game's technology database), slot +0x30 (0x2f71),
     * bytes +0x34 (0x3c0a: spend XP to boost; the template must be boostable), +0x35 (0x456f), +0x36 (0x33e3: UI feedback
     * only). The view's helper builds it with the UI constructor (1.20 0x100200640, 1.19.4.1 0x1001fff50) and the
     * slot-selected click passes (0, 1, view mode); the factory's own defaults are the same (0, 1, 0). The module posts
     * (player tag, template, slot, 0, 1, 0). The "can do": the slot exists (0 <= slot < the country's slot count) and the
     * country can research the template now (prerequisites, availability, not done, no exclusive alternative). An
     * exclusive pair is switched by another command (CSetXORResearchCommand): refused here, the mouse path serves it. */
    {"research", "research.start", "CSetResearchCommand", 0x38, BUILD_CTOR, CTOR_RESEARCH, 0, 0x24, "19CTechnologyTemplate", 6,
     {{"tag", 0x24, FK_TAG, 0, 0x289a, 0}, {"technology", 0x28, FK_PTR, 0, 0x285f, 0}, {"slot", 0x30, FK_I32, 0, 0x2f71, -1, 1},
      {"xp_boost", 0x34, FK_U8, 0, 0x3c0a, 0}, {"b35", 0x35, FK_U8, 0, 0x456f, 1}, {"ui", 0x36, FK_U8, 0, 0x33e3, 0}},
     OBJ2_NONE, 0, AUTH_TAG_PTR, "19CTechnologyTemplate"},
    /* ---- politics (engineer 79, 24 Nov): laws and other idea slots, as the politics view posts them (UI 1.20 0x10234a8c0:
     * the selection item's click). Read from 1.20.0.0 and 1.19.4.1 (offsets identical). Ideas are raw CIdea pointers of the
     * game's idea database (TGameItemDatabase<CIdeaDatabase>, ideas +0x60/+0x6c, the serialized token +0x8); the
     * country's active ideas are country+0xd38 -> +0x50/+0x5c; an idea's category +0x908. The view: the slot's current
     * idea active -> CReplaceIdeaCommand 13017 (0x38; ctor 1.20 0x100209e40: tag +0x24 (0x289a), old +0x28 (0x332e), new
     * +0x30 (0x331a)) behind its confirmation popup; else CAddIdeaCommand 12303 (0x30; ctor 0x100206170: tag, idea +0x28
     * (0x28fb)). The "can do" (replace 0x10020a3c0): both real ideas, the SAME category (+0x908), the old one active and
     * the new one not, the political power cost (0x1013d7d50), the old one removable (0x1013de680), the new one allowed
     * (0x1013dac30, 0x1013de2e0); add 0x100206c20: not active, cost, allowed. */
    {"politics", "politics.add", "CAddIdeaCommand", 0x30, BUILD_CTOR, CTOR_IDEA_ADD, 0, 0x24, "5CIdea", 2,
     {{"tag", 0x24, FK_TAG, 0, 0x289a, 0}, {"idea", 0x28, FK_PTR, 0, 0x28fb, 0}}, OBJ2_NONE, 0, AUTH_TAG_PTR, "5CIdea"},
    {"politics", "politics.replace", "CReplaceIdeaCommand", 0x38, BUILD_CTOR, CTOR_IDEA_REPLACE, 0, 0x24, "5CIdea", 3,
     {{"tag", 0x24, FK_TAG, 0, 0x289a, 0}, {"old", 0x28, FK_PTR, 0, 0x332e, 0}, {"idea", 0x30, FK_PTR, 0, 0x331a, 0}},
     OBJ2_SAME_PTR, 0, AUTH_TAG_PTR, "5CIdea"},
    /* ---- construction (engineer 82, 24 Nov): the construction view's three posts, read from 1.20.0.0 and 1.19.4.1
     * (identical code and field ids).
     * add: the map click in construction mode (UI 1.20 0x1019d2630; the state view's build-max 0x10225cf90 posts the same
     * command with the game's own free-slot count): AddConstruction(mode, &own tag, &reference, count) through the
     * click's own constructor (1.20 0x1001f9ef0, 1.19.4.1 0x1001f97b0). CAddConstructionCommand 12151 (0x48): tag +0x24
     * (0x289a), a CBuildingReference by value +0x28 (0x284f: vptr; location id +0x30 (0x286d), building id +0x34
     * (0x4c1a), tag +0x38 (0x289a, 0 for a state or province location)), count +0x40 (0x2f78), mode +0x44 (0x8d: the
     * queue placement of the new line; the plain click passes 2). The "can do" (1.20 0x1001fa840): the tag is a
     * country, the reference names a building, its location resolves, the country may build it there now
     * (0x100dc0770(location, template, &tag, 1): slots, ownership, the building's own conditions), mode < 4. The module
     * posts one command per click (count 1, mode 2), as a player clicking `count` times. */
    {"construction", "construction.add", "CAddConstructionCommand", 0x48, BUILD_CTOR, CTOR_CONSTRUCTION, 0, 0x24, "17CBuildingTemplate", 4,
     {{"tag", 0x24, FK_TAG, 0, 0x289a, 0}, {"reference", 0x28, FK_I32, 0, 0x284f, 0}, {"count", 0x40, FK_I32, 0, 0x2f78, 0},
      {"mode", 0x44, FK_U8, 0, 0x8d, 0}}, OBJ2_NONE, 0, AUTH_TAG_PTR, "17CBuildingTemplate"},
    /* remove: CRemoveConstructionCommand 13088 (0x30): the line's handle +0x24 (0x2fb3), levels +0x2c (0x1a1). The row's
     * close button posts it with 1 level (behind a confirmation when one level is left), the map's removal with the
     * levels it takes away (ctor 1.20 0x1001faf60 (cmd, line, levels)); a building line loses the levels, a line with
     * none left leaves the queue. Authorisation: the line resolves as a CGeneralProductionLine owned by the sender's
     * country; the "can do": the line resolves. The module posts the line's own level count (the whole line). */
    {"construction", "construction.remove", "CRemoveConstructionCommand", 0x30, BUILD_FIELDS, CTOR_NONE, 0, 0x24,
     "22CGeneralProductionLine", 2, {{"line", 0x24, FK_REF, 0, 0x2fb3, 0}, {"levels", 0x2c, FK_I32, 0, 0x1a1, 0}}, OBJ2_NONE, 0},
    /* move: the construction view's own move (CCountryConstructionsView slot 0xc0 (from, to), behind the rows'
     * increase/decrease priority buttons; Shift = to the top / bottom): CSetProductionLinePlacementCommand 17412 (0x38;
     * ctor 1.20 0x1001fe090 (cmd, line, index, grouped)): the line's handle +0x24 (0x2fb3), the new index +0x2c (0x2f72),
     * the line's "placed by hand" byte +0x30 (0x32ed) the view computes from the neighbours at the new place (1.20
     * 0x10204cc20: 0 when the line joins a group of the same building, else 1). Its execution moves the line and stores
     * the byte in the line (+0x70). The MCP computes the byte by the view's rule from the queue it reads. */
    {"construction", "construction.move", "CSetProductionLinePlacementCommand", 0x38, BUILD_FIELDS, CTOR_NONE, 0, 0x24,
     "22CGeneralProductionLine", 3, {{"line", 0x24, FK_REF, 0, 0x2fb3, 0}, {"index", 0x2c, FK_I32, 0, 0x2f72, -1, 1},
      {"grouped", 0x30, FK_U8, 0, 0x32ed, 0, 2}}, OBJ2_NONE, 0},
    {"holds", "holds.area_defense", "CEditAreaDefenseStateCommand", 0x50, BUILD_CTOR, CTOR_AREA_STATE, 0, 0x24, "12COrdersGroup", 5,
     {{"states", 0x30, FK_I32_ARRAY, 0x3c, 0x1b7, 0}, {"group", 0x24, FK_REF, 0, 0x3f, 0}, {"order_id", 0x2c, FK_I32, 0, 0x3036, 0},
      {"settings", 0x4c, FK_I32, 0, 0x2b5d, 0}, {"replace", 0x48, FK_U8, 0, 0x29f1, 0}}, OBJ2_NONE, 0},
};
const int CMD2_NSPECS = (int)(sizeof CMD2_SPECS / sizeof CMD2_SPECS[0]);

static uint32_t rd32(const uint8_t *p) { uint32_t v; memcpy(&v, p, 4); return v; }
static int32_t rds32(const uint8_t *p) { int32_t v; memcpy(&v, p, 4); return v; }
static int in_text(const scan_image *im, uint64_t a) { return a >= im->text_addr && a < im->text_addr + im->text_size; }

/* first match of pat in [a, a+len); caps of that match */
static long pfind_at(const scan_image *im, uint64_t a, size_t len, const char *pat, uint32_t *caps, int maxcaps, unsigned *count) {
    const uint8_t *b = scan_ptr(im, a, len);
    long first = -1; unsigned c = 0; uint32_t tmp[8];
    if (!b) { if (count) *count = 0; return -1; }
    for (size_t i = 0; i < len; i++)
        if (cmd_pmatch(b + i, len - i, pat, first < 0 ? caps : tmp, maxcaps)) { if (first < 0) first = (long)i; c++; }
    if (count) *count = c;
    return first;
}

static uint64_t find_cstr(const scan_image *im, const char *s) {
    const uint8_t *b = scan_ptr(im, im->cstr_addr, im->cstr_size);
    size_t n = strlen(s) + 1;
    if (!b || im->cstr_size < n) return 0;
    for (uint64_t i = 0; i + n <= im->cstr_size; i++)
        if (b[i] == (uint8_t)s[0] && !memcmp(b + i, s, n) && (i == 0 || b[i - 1] == 0)) return im->cstr_addr + i;
    return 0;
}

/* every "lea r64,[rip+target]" in __text (up to max); returns the count */
static unsigned lea_sites(const scan_image *im, uint64_t target, uint64_t *out, unsigned max) {
    const uint8_t *t = scan_ptr(im, im->text_addr, im->text_size);
    unsigned n = 0;
    if (!t) return 0;
    for (uint64_t i = 0; i + 7 <= im->text_size; i++)
        if ((t[i] == 0x48 || t[i] == 0x4c) && t[i + 1] == 0x8d && (t[i + 2] & 0xc7) == 0x05 &&
            im->text_addr + i + 7 + (int64_t)rds32(t + i + 3) == target) {
            if (n < max) out[n] = im->text_addr + i;
            n++;
        }
    return n;
}

/* start of the function containing a (function-starts table) */
static uint64_t fn_start(const scan_image *im, uint64_t a) {
    if (!im->fstarts || !im->nfstarts) return 0;
    size_t lo = 0, hi = im->nfstarts;
    while (hi - lo > 1) { size_t mid = (lo + hi) / 2; if (im->fstarts[mid] <= a) lo = mid; else hi = mid; }
    return im->fstarts[lo] <= a ? im->fstarts[lo] : 0;
}

/* Itanium RTTI: the name of the typeinfo object at ti (ti+8 -> name) */
static int ti_name(const scan_image *im, uint64_t ti, char *out, size_t n) {
    uint64_t nm; const uint8_t *s;
    out[0] = 0;
    if (!scan_q(im, ti + 8, &nm) || !(s = scan_ptr(im, nm, 64))) return 0;
    size_t k = 0;
    while (k + 1 < n && k < 63 && s[k] >= 0x20 && s[k] < 0x7f) { out[k] = (char)s[k]; k++; }
    out[k] = 0;
    return k > 0 && s[k] == 0;
}

int scan_stage2_common(const scan_image *im, const post_layout *pl, uint64_t gs_global, stage2_common *o) {
    memset(o, 0, sizeof *o);
    uint32_t c[8]; unsigned n;
    char *ev = o->evidence; size_t evn = sizeof o->evidence;
    if (!pl || !pl->ok) { snprintf(ev, evn, "post layout unresolved"); return 0; }
    /* 1. the "can do" slot and the deleting destructor, from the AI post helper (same calls as the UI helpers) */
    uint64_t s = find_cstr(im, "AI tried to post an invalid command: %s"), site[4];
    if (!s || lea_sites(im, s, site, 4) != 1) { snprintf(ev, evn, "AI post helper string not referenced once"); return 0; }
    uint64_t helper = fn_start(im, site[0]);
    if (!helper || site[0] - helper > 0x400) { snprintf(ev, evn, "AI post helper start unknown"); return 0; }
    long v = pfind_at(im, helper, site[0] - helper, "48 8b 03 48 89 df 31 f6 ff 50 B", c, 8, &n);
    if (v < 0 || n != 1) { snprintf(ev, evn, "validity call in the post helper: %u matches", n); return 0; }
    o->valid_slot = c[0];
    long p = pfind_at(im, helper, site[0] - helper, "48 89 c7 48 89 de ba 01 00 00 00 e8 D", c, 8, &n); /* Post(q, cmd, 1) */
    uint64_t post_call = p >= 0 ? helper + (uint64_t)p + 16 + (int64_t)(int32_t)c[0] : 0;
    long d = pfind_at(im, site[0], 0x80, "48 8b 03 48 89 df ff 50 B", c, 8, &n);
    o->delete_slot = d >= 0 ? c[0] : 0;
    /* 2. Post's two drop conditions for gameplay commands: the executing byte and its nested-post override */
    if (pfind_at(im, pl->post, 0x300, "41 80 be D 00 74 ?? 41 80 be D 00", c, 8, &n) < 0 || n != 1) {
        snprintf(ev, evn, "executing-commands check not found in Post"); return 0;
    }
    o->exec_flag_off = c[0]; o->allow_flag_off = c[1];
    /* 3. the per-frame gate: session[gate] = (gamestate[player tag] > 0); every site must agree */
    unsigned votes = 0, other = 0;
    const uint8_t *t = scan_ptr(im, im->text_addr, im->text_size);
    for (uint64_t i = 7; t && i + 18 <= im->text_size; i++) {
        if (t[i] != 0x48 || t[i + 1] != 0x8b || t[i + 2] != 0x00 || t[i + 3] != 0x83 || t[i + 4] != 0xb8 || t[i + 9] != 0) continue;
        if (!(cmd_pmatch(t + i + 10, 8, "41 0f 9f 86 D", c, 8) || cmd_pmatch(t + i + 10, 8, "0f 9f 83 D", c, 8))) continue;
        if (c[0] != pl->gate_flag_off) continue;
        if (t[i - 6] != 0x8d || (t[i - 5] & 0xc7) != 0x05 || im->text_addr + i + (int64_t)rds32(t + i - 4) != gs_global) continue;
        uint32_t tag = rd32(t + i + 5);
        if (!o->player_tag_off) o->player_tag_off = tag;
        if (tag == o->player_tag_off) votes++; else other++;
    }
    o->gate_flag_off = pl->gate_flag_off;
    /* 4. the empty handle, the handle offset inside a reference object, the line's factory field: from the
     *    production commands (factory, the UI constructor, the game's own check) */
    scan_cap f, vt, tid;
    uint64_t lf_vt = 0;
    if (scan_command_factory(im, "CAddProductionLineFactoriesCommand", &f, &vt, &tid)) {
        lf_vt = vt.value;
        long z = pfind_at(im, f.value, 0x60, "48 8d 0d D 48 8b 09 48 89 48 24", c, 8, &n);
        if (z >= 0) o->null_ref_global = f.value + (uint64_t)z + 7 + (int64_t)(int32_t)c[0];
        uint64_t valid;
        if (scan_q(im, vt.value + o->valid_slot, &valid) && in_text(im, valid) &&
            pfind_at(im, valid, 0xb0, "48 63 88 D 48 63 5b 2c", c, 8, &n) >= 0 && n == 1)
            o->line_factories_off = c[0];
    }
    if (lf_vt) { /* the UI constructor: header, vptr, empty handle, then handle = [line + H] */
        uint64_t st[8]; unsigned ns = lea_sites(im, lf_vt, st, 8);
        for (unsigned i = 0; i < ns && i < 8; i++)
            if (pfind_at(im, st[i], 32, "48 89 07 48 8d 05 D 48 8b 00 48 89 47 24 48 8b 46 B 48 89 47 24", c, 8, &n) == 7) {
                o->cref_handle_off = c[1]; break;
            }
    }
    /* 5. the strategic region id the air commands carry: Transfer's UI constructor reads it from the region object */
    if (scan_command_factory(im, "CStratAirTransferCommand", &f, &vt, &tid)) {
        uint64_t st[8]; unsigned ns = lea_sites(im, vt.value, st, 8);
        for (unsigned i = 0; i < ns && i < 8 && !o->region_id_off; i++) {
            uint64_t fs = fn_start(im, st[i]);
            if (fs && pfind_at(im, fs, 0x1a0, "4d 85 ff 74 ?? 41 8b 47 B eb ?? 31 c0 41 89 46 38", c, 8, &n) >= 0 && n == 1)
                o->region_id_off = c[0];
        }
    }
    /* 6. (engineer 71) a strategic region by id, as EnableMission's own "can do" reads it:
     *    mov eax,[cmd+0x58]; test; jle; lea rcx,[gamestate global]; mov rcx,[rcx]; cmp [rcx+CNT],eax; jle;
     *    mov rcx,[rcx+ARR]; mov r15,[rcx+8*rax]. The global must be the game-state global of the stage-1 scan. */
    if (scan_command_factory(im, "CStratAirEnableMissionCommand", &f, &vt, &tid)) {
        uint64_t valid;
        if (scan_q(im, vt.value + o->valid_slot, &valid) && in_text(im, valid)) {
            long r = pfind_at(im, valid, 0x400, "8b 43 58 85 c0 7e ?? 48 8d 0d D 48 8b 09 39 81 D 7e ?? 48 8b 89 D 4c 8b 3c c1", c, 8, &n);
            if (r >= 0 && n == 1 && valid + (uint64_t)r + 14 + (int64_t)(int32_t)c[0] == gs_global && c[1] < 0x4000 && c[2] < 0x4000) {
                o->regions_cnt_off = c[1]; o->regions_arr_off = c[2];
            }
        }
    }
    /* 7. (engineer 71) the army's cast, as the attach command's own "can do" makes it: __dynamic_cast(obj,
     *    CReferenceObject, COrdersGroup) (lea rsi, [typeinfo]; lea rdx, [typeinfo]), names checked by RTTI */
    if (scan_command_factory(im, "CAttachAirWingToArmyCommand", &f, &vt, &tid)) {
        uint64_t valid;
        if (scan_q(im, vt.value + o->valid_slot, &valid) && in_text(im, valid)) {
            const uint8_t *vb = scan_ptr(im, valid, 0x200);
            for (size_t k = 0; vb && k + 14 <= 0x200 && !o->ti_orders_group; k++) {
                if (!cmd_pmatch(vb + k, 14, "48 8d 35 D 48 8d 15 D", c, 8)) continue;
                uint64_t a = valid + k, src = a + 7 + (int64_t)(int32_t)c[0], dst = a + 14 + (int64_t)(int32_t)c[1];
                char sn[64], dn[64];
                if (ti_name(im, src, sn, sizeof sn) && !strcmp(sn, "16CReferenceObject") && ti_name(im, dst, dn, sizeof dn) &&
                    !strcmp(dn, "12COrdersGroup")) { o->ti_reference = src; o->ti_orders_group = dst; }
            }
        }
    }
    /* 8. (engineer 74) the CPdxArray allocator's allocate slot: every growth site "mov rax,[rdi]; mov edx,8; call
     *    [rax+SLOT]" must agree (thousands of sites) */
    {
        unsigned sites = 0, dis = 0; uint32_t slot = 0;
        for (uint64_t i = 0; t && i + 11 <= im->text_size; i++) {
            if (!cmd_pmatch(t + i, 11, "48 8b 07 ba 08 00 00 00 ff 50 B", c, 8)) continue;
            if (!slot) slot = c[0];
            if (c[0] == slot) sites++; else dis++;
        }
        if (sites >= 100 && !dis) o->alloc_slot = slot;
    }
    /* 9. (engineer 74) a division as COrderAssignCommand's own check reads it: the handle resolves, the cast
     *    (CReferenceObject -> CUnit, hint H) succeeds, then the unit's army at [unit + G] (lea rcx,[rax-H] ... [rcx+G]) */
    if (scan_command_factory(im, "COrderAssignCommand", &f, &vt, &tid)) {
        uint64_t valid;
        if (scan_q(im, vt.value + o->valid_slot, &valid) && in_text(im, valid)) {
            long a = pfind_at(im, valid, 0x200, "b9 B 00 00 00 48 89 c7 48 8d 35 D 48 8d 15 D e8 D", c, 8, &n);
            if (a >= 0 && n == 1) {
                uint64_t at = valid + (uint64_t)a + 8, src = at + 7 + (int64_t)(int32_t)c[1], dst = at + 14 + (int64_t)(int32_t)c[2];
                char sn[64], dn[64];
                uint32_t hint = c[0];
                if (ti_name(im, src, sn, sizeof sn) && !strcmp(sn, "16CReferenceObject") && ti_name(im, dst, dn, sizeof dn) &&
                    !strcmp(dn, "5CUnit") &&
                    pfind_at(im, valid, 0x200, "48 8d 48 B 48 85 c0 48 0f 44 c8 48 8b 81 D 48 85 c0", c, 8, &n) >= 0 && n == 1 &&
                    (uint8_t)(0x100 - c[0]) == hint && c[1] < 0x1000) {
                    o->ti_unit = dst; o->unit_hint = hint; o->unit_group_off = c[1];
                }
            }
        }
    }
    scan_cap rv, bv;
    if (scan_class_vtable(im, "16CStrategicRegion", &rv)) o->vt_region = rv.value;
    if (scan_class_vtable(im, "8CAirBase", &bv)) o->vt_base = bv.value;
    o->ok = o->vt_region && o->vt_base && o->valid_slot && o->valid_slot % 8 == 0 && o->valid_slot < 0x100 && o->delete_slot == 8 && post_call == pl->post &&
            o->exec_flag_off && o->exec_flag_off < 0x4000 && o->allow_flag_off && o->allow_flag_off < 0x4000 &&
            votes >= 2 && other == 0 && o->player_tag_off && o->player_tag_off < 0x4000 && o->null_ref_global &&
            o->cref_handle_off == 8;
    snprintf(ev, evn,
             "valid slot 0x%x, delete slot 0x%x (AI helper %s Post); executing [s+0x%x] override [s+0x%x]; gate = "
             "gs+0x%x > 0 (%u sites, %u disagree); empty handle 0x%llx; handle +0x%x; line factories +0x%x; region id +0x%x",
             o->valid_slot, o->delete_slot, post_call == pl->post ? "calls" : "does NOT call", o->exec_flag_off,
             o->allow_flag_off, o->player_tag_off, votes, other, (unsigned long long)o->null_ref_global, o->cref_handle_off,
             o->line_factories_off, o->region_id_off);
    return o->ok;
}

/* the serializer writes field id `id` (mov esi, id) with the field's offset close to it: as an 8-bit ModRM displacement
 * ([reg + off]) or as "add reg, off" (a field passed by address). A list may be addressed by its data or its count
 * offset (off2). Window: 40 bytes either side (engineer 71: the disband serializer loads the list 0x22 bytes before). */
static int serial_has2(const scan_image *im, uint64_t fn, uint32_t id, uint16_t off, uint16_t off2) {
    const uint8_t *b = scan_ptr(im, fn, 0x200);
    if (!b) return 0;
    for (size_t i = 0; i + 5 <= 0x200; i++) {
        if (b[i] != 0xbe || rd32(b + i + 1) != id) continue;
        for (size_t j = i > 40 ? i - 40 : 0; j < i + 40 && j + 2 < 0x200; j++) {
            if (j == i) continue;
            uint8_t d = b[j + 1];
            if ((d == off || (off2 && d == off2)) && d < 0x80 && (b[j] & 0xc0) == 0x40) return 1;
            /* engineer 76: a field past +0x7f is addressed with a 32-bit displacement ([reg + 0xe8]) */
            if (off >= 0x80 && (b[j] & 0xc0) == 0x80 && (b[j] & 7) != 4 && j + 5 <= 0x200 && rd32(b + j + 1) == off) return 1;
            if ((b[j] == 0x48 || b[j] == 0x49) && b[j + 1] == 0x83 && (b[j + 2] & 0xf8) == 0xc0 && j + 3 < 0x200 &&
                (b[j + 3] == off || (off2 && b[j + 3] == off2)))
                return 1;
        }
        return 0;
    }
    return 0;
}
static int serial_has(const scan_image *im, uint64_t fn, uint32_t id, uint16_t off) { return serial_has2(im, fn, id, off, 0); }

/* the auth slot's shape: typed resolver, owner getter, "sender controls the country" check */
static int auth_shape(const scan_image *im, uint64_t fn, const cmd_spec *s, cmd_type *o) {
    uint32_t c[8]; unsigned n;
    uint64_t resolve = 0, owner = 0, controls = 0;
    if (!s->auth_array) {
        long p = pfind_at(im, fn, 0x40, "48 83 c7 B e8 D 48 89 c7 e8 D 89 45 B 48 8d 75 B 4c 89 f7 48 89 da e8 D", c, 8, &n);
        if (p < 0 || c[0] != s->auth_off) return 0;
        uint64_t at = fn + (uint64_t)p;
        resolve = at + 9 + (int64_t)(int32_t)c[1];
        owner = at + 17 + (int64_t)(int32_t)c[2];
        controls = at + 35 + (int64_t)(int32_t)c[5];
    } else {
        long p = pfind_at(im, fn, 0x60, "49 8b 7e B e8 D 48 89 c7 e8 D 89 45 B 48 8d 75 B 4c 89 f7 48 89 da e8 D", c, 8, &n);
        if (p < 0) p = pfind_at(im, fn, 0x60, "48 8b 7f B e8 D 48 89 c7 e8 D 89 45 B 48 8d 75 B 4c 89 f7 48 89 da e8 D", c, 8, &n);
        if (p < 0 || c[0] != s->auth_off) return 0;
        uint64_t at = fn + (uint64_t)p;
        resolve = at + 9 + (int64_t)(int32_t)c[1];
        owner = at + 17 + (int64_t)(int32_t)c[2];
        controls = at + 35 + (int64_t)(int32_t)c[5];
    }
    /* the owner getter: a null check returning 0, then a short read-only body (an inline pointer chain or one call
     * plus a load) ending in pop rbp; ret */
    const uint8_t *ob = scan_ptr(im, owner, 0x40);
    if (!ob || !cmd_pmatch(ob, 0x40, "48 85 ff 74 B 55 48 89 e5", c, 8) || c[0] < 6 || c[0] > 0x30) return 0;
    if (!cmd_pmatch(ob + 5 + c[0], 3, "31 c0 c3", c + 1, 1) || !cmd_pmatch(ob + 5 + c[0] - 2, 2, "5d c3", c + 1, 1)) return 0;
    if (!in_text(im, resolve) || !in_text(im, controls)) return 0;
    o->resolve_fn = resolve; o->owner_fn = owner; o->controls_fn = controls;
    return 1;
}

/* the typed resolver: lookup(&handle) then __dynamic_cast(obj, CReferenceObject, target, hint) */
static int resolver_shape(const scan_image *im, const cmd_spec *s, cmd_type *o) {
    uint32_t c[8]; unsigned n;
    long a = pfind_at(im, o->resolve_fn, 0x90, "48 89 df e8 D 48 85 c0", c, 8, &n);
    if (a < 0) return 0;
    o->lookup_fn = o->resolve_fn + (uint64_t)a + 8 + (int64_t)(int32_t)c[0];
    long b = pfind_at(im, o->resolve_fn, 0x90, "48 8d 35 D 48 8d 15 D", c, 8, &n);
    if (b < 0 || n != 1) return 0;
    uint64_t at = o->resolve_fn + (uint64_t)b, src = at + 7 + (int64_t)(int32_t)c[0], dst = at + 14 + (int64_t)(int32_t)c[1];
    char sn[64], dn[64];
    if (!ti_name(im, src, sn, sizeof sn) || strcmp(sn, "16CReferenceObject") || !ti_name(im, dst, dn, sizeof dn) ||
        strcmp(dn, s->target))
        return 0;
    const uint8_t *h = scan_ptr(im, at + 14, 16);
    if (!h) return 0;
    if (cmd_pmatch(h, 16, "b9 D", c, 8)) o->target_hint = c[0];
    else if (cmd_pmatch(h, 16, "48 89 c7 31 c9", c, 8) || cmd_pmatch(h, 16, "31 c9", c, 8)) o->target_hint = 0;
    else if (cmd_pmatch(h, 16, "45 31 ff 48 89 c7 31 c9", c, 8)) o->target_hint = 0;
    else return 0;
    const uint8_t *lk = scan_ptr(im, o->lookup_fn, 16);
    return lk && o->target_hint < 0x400 && cmd_pmatch(lk, 16, "55 48 89 e5 8b 07 48 3d", c, 8);
}

/* the UI constructor for BUILD_CTOR types: stores the vptr and the arguments at the serializer's offsets */
static int find_ctor(const scan_image *im, const cmd_spec *s, cmd_type *o) {
    uint64_t st[16]; unsigned ns = lea_sites(im, o->vtable, st, 16);
    uint32_t c[8];
    for (unsigned i = 0; i < ns && i < 16; i++) {
        uint64_t fs = fn_start(im, st[i]);
        const uint8_t *pre = fs ? scan_ptr(im, fs, 0x40) : NULL, *body = scan_ptr(im, st[i], 0x60);
        if (!pre || !body || st[i] - fs > 0x60) continue;
        if (s->ctor == CTOR_WING_I32_U8 &&
            cmd_pmatch(pre, 0x40, "55 48 89 e5 41 57 41 56 53 50 48 89 75 e0", c, 8) &&
            cmd_pmatch(body, 0x60, "48 8d 05 D 48 89 07 0f 57 c0 0f 11 47 28 48 8b 05 D 48 89 47 38 89 57 40 88 4f 44 "
                                   "66 c7 47 45 00 00 48 85 f6 74 ?? 48 89 fb 4c 8d 7f 28 48 8d 75 e0 4c 89 ff e8 D", c, 8)) {
            o->ctor = fs; return 1;
        }
        if (s->ctor == CTOR_WING_I32_NULL &&
            cmd_pmatch(pre, 0x40, "55 48 89 e5 41 57 41 56 41 55 41 54 53 48 83 ec 18 49 89 cc 48 89 fb 48 89 75 c8 48 89 4d d0", c, 8) &&
            cmd_pmatch(body, 0x60, "48 8d 05 D 48 89 07 4c 8d 7f 28 4c 8d 77 38 0f 57 c0 0f 11 47 28 48 8b 05 D 48 89 47 38 "
                                   "4c 8d 6f 40 0f 11 47 40 48 89 47 50 89 57 58 c6 47 5c 00 48 8d 75 c8 4c 89 ff e8 D 4d 85 e4", c, 8)) {
            o->ctor = fs; return 1;
        }
        /* engineer 71: the rest of the air family. Each shape = the stores after the vptr that bind the arguments,
         * read from the 1.19.4.1 and 1.20.0.0 disassembly (identical code): the list (+0x28 data, +0x38 allocator),
         * then where each argument goes. The single-object ctors stash the handle [arg + 0x18] (an air wing's handle:
         * its reference base +0x10, handle +8) for the list append; the list ctors read the array's size at +0xc. */
        const char *L = "48 8d 05 D 48 89 07 0f 57 c0 0f 11 47 28 48 8b 05 D 48 89 47 38 ";
        char pat[300];
        if (s->ctor == CTOR_OBJ) {
            snprintf(pat, sizeof pat, "%s48 8d 05 D 48 8b 00 48 89 45 d0 48 85 f6 74 ?? 48 8b 46 18", L);
            if (cmd_pmatch(pre, 4, "55 48 89 e5", c, 8) && cmd_pmatch(body, 0x60, pat, c, 8)) { o->ctor = fs; return 1; }
        }
        if (s->ctor == CTOR_TAGP_OBJ) { /* the tag by reference into +0x40, then the wing (rdx) */
            snprintf(pat, sizeof pat, "%s8b 06 89 47 40 48 8d 05 D 48 8b 00 48 89 45 d0 48 85 d2 74 ?? 48 8b 42 18", L);
            if (cmd_pmatch(pre, 4, "55 48 89 e5", c, 8) && cmd_pmatch(body, 0x60, pat, c, 8)) { o->ctor = fs; return 1; }
        }
        if (s->ctor == CTOR_OBJ_OBJ2) { /* the army's handle [rdx + 8] into +0x40; the wing (rsi kept in r14) read later */
            snprintf(pat, sizeof pat, "%s4c 8d 25 D 49 8b 04 24 48 89 47 40 48 85 d2 74 ?? 48 8d 7b 40 48 8b 42 08 48 89 07", L);
            const uint8_t *all = scan_ptr(im, fs, 0x200);
            uint32_t c2[2]; int wing_read = 0;
            for (size_t k = 0; all && k + 4 <= 0x200; k++) if (cmd_pmatch(all + k, 4, "49 8b 46 18", c2, 2)) { wing_read = 1; break; }
            if (cmd_pmatch(pre, 0x20, "55 48 89 e5 41 57 41 56 41 54 53 48 83 ec 10 49 89 f6", c, 8) &&
                cmd_pmatch(body, 0x60, pat, c, 8) && wing_read) { o->ctor = fs; return 1; }
        }
        if (s->ctor == CTOR_ADDLINE && /* tag by reference into +0x24, the empty handle into +0x28, then [rdx + 8] */
            cmd_pmatch(pre, 0x14, "55 48 89 e5 41 57 41 56 41 55 41 54 53 50 49 89 cf 49 89 fe", c, 8) &&
            cmd_pmatch(body, 0x40, "48 8d 05 D 48 89 07 8b 06 89 47 24 48 8d 1d D 48 8b 03 48 89 47 28 48 8b 42 08 48 89 47 28", c, 8)) {
            o->ctor = fs; return 1;
        }
        /* engineer 73: SetPath's UI constructor (a thunk jumps to it): flag in r15d, provinces r14, order r12, then the
         * vptr, the group's handle [[order + owner] + 8] and the order's id into +0x2c */
        if (s->ctor == CTOR_ORDER_PATH &&
            cmd_pmatch(pre, 0x1e, "55 48 89 e5 41 57 41 56 41 55 41 54 53 50 41 89 cf 49 89 d6 49 89 f4 48 89 fb", c, 8) &&
            cmd_pmatch(body, 0x10, "48 8d 05 D 48 89 07 48 8b 46 38", c, 8)) {
            o->ctor = fs; return 1;
        }
        /* NewFront's UI constructor: the divisions pointer kept at [rbp-0x38], parent r13, provinces r12, the drawing
         * group from the stack, then the vptr and the group's handle into +0x24 */
        if (s->ctor == CTOR_NEW_FRONT &&
            cmd_pmatch(pre, 0x22, "55 48 89 e5 41 57 41 56 41 55 41 54 53 48 83 ec 48 4c 89 45 c8 49 89 cd 49 89 d4 48 89 fb 4c 8b 7d 20", c, 8) &&
            cmd_pmatch(body, 0x18, "48 8d 05 D 48 89 07 4c 8d 35 D 49 8b 06 48 89 47 24", c, 8)) {
            const uint8_t *all = scan_ptr(im, fs, 0x200);
            uint32_t c2[4]; int flags = 0;
            for (size_t k = 0; all && k + 0x30 <= 0x200 && !flags; k++)
                if (cmd_pmatch(all + k, 0x30, "44 88 4b 6c 40 88 73 6d", c2, 4)) flags = 1;
            if (flags) { o->ctor = fs; return 1; }
        }
        /* engineer 75: army structure. Each = the prologue that binds the arguments (read in both builds) plus one
         * store or call that is that constructor's own (searched anywhere in its first 0x200 bytes) */
        static const struct { int kind; const char *pre, *any; } UC[] = {
            {CTOR_DIVS_THEATRE, "55 48 89 e5 41 57 41 56 41 55 41 54 53 48 83 ec 18 49 89 cc 49 89 d6 49 89 f7 48 89 fb", "4c 89 fe 4c 89 f2 e8"},
            {CTOR_LEADER, "55 48 89 e5 41 57 41 56 41 55 41 54 53 50 89 cb 49 89 d7 49 89 fe", "41 88 5e 34"},
            {CTOR_GROUP_NAME, "55 48 89 e5 41 57 41 56 41 54 53 48 83 ec 10 49 89 d6 48 89 fb", "48 8d 7b 30 41 f6 06 01"},
            {CTOR_GROUPS_OBJ, "55 48 89 e5 41 57 41 56 41 55 41 54 53 48 83 ec 28 49 89 f7 48 89 fb", "4c 8d 73 30"},
            {CTOR_GROUPS_OBJ_I32, "55 48 89 e5 41 57 41 56 41 55 41 54 53 48 83 ec 28 41 89 cf 49 89 f4 48 89 fb", "44 89 7b 48"},
            {CTOR_GROUPS, "55 48 89 e5 41 57 41 56 41 55 41 54 53 48 83 ec 28 c6 47 08 00", "49 89 f4"},
            /* engineer 79: research: the stack byte first, then tag (by reference), template, slot and the three bytes */
            {CTOR_RESEARCH, "55 48 89 e5 0f b6 45 10 c6 47 08 00", "44 88 4f 35 88 47 36"},
            /* engineer 79: the idea commands: no frame, the tag by reference, then the pointers (and nothing else); <= 12 bytes */
            {CTOR_IDEA_ADD, "55 48 89 e5 c6 47 08 00", "8b 06 89 47 24 48 89 57 28 5d c3"},
            {CTOR_IDEA_REPLACE, "55 48 89 e5 c6 47 08 00", "89 47 24 48 89 57 28 48 89 4f 30 5d"},
            /* engineer 82: construction: no frame, the tag by reference into +0x24, the reference's location/building/tag
             * copied into +0x30..+0x3b, count r8d +0x40, mode sil +0x44 */
            {CTOR_CONSTRUCTION, "55 48 89 e5 c6 47 08 00", "48 89 47 30 8b 41 10 89 47 38 44 89"},
            /* engineer 76: the event answer: option r9d -> ebx, scope r8 -> r12, country * rcx -> r14, cmd -> r13; the
             * option index stored at +0xe8 (the serializer's offset of field 0x2966) */
            {CTOR_EVENT_OPTION, "55 48 89 e5 41 57 41 56 41 55 41 54 53 48 83 ec 18 44 89 cb 4d 89 c4 49 89 ce 49 89 fd",
             "41 89 9d e8 00 00 00"},
        };
        /* engineer 79: the area-defence state click: settings r8d -> r15d, state ecx -> [rbp-0x30], order id edx -> r13d,
         * group rsi -> r12; then the order id into +0x2c and the replace/has-settings word 0x100 into +0x48 */
        if (s->ctor == CTOR_AREA_STATE &&
            cmd_pmatch(pre, 0x1d, "55 48 89 e5 41 57 41 56 41 55 41 54 53 50 45 89 c7 89 4d d0 41 89 d5 49 89 f4 48 89 fb", c, 8)) {
            const uint8_t *all = scan_ptr(im, fs, 0x200);
            int idw = 0, word = 0, set = 0;
            for (size_t k = 0; all && k + 6 <= 0x200; k++) {
                if (cmd_pmatch(all + k, 4, "44 89 6b 2c", c, 8)) idw = 1;
                if (cmd_pmatch(all + k, 6, "66 c7 43 48 00 01", c, 8)) word = 1;
                if (cmd_pmatch(all + k, 4, "44 89 7b 4c", c, 8)) set = 1;
            }
            if (idw && word && set) { o->ctor = fs; return 1; }
        }
        for (size_t u = 0; u < sizeof UC / sizeof UC[0]; u++) {
            if (s->ctor != UC[u].kind || !cmd_pmatch(pre, 0x40, UC[u].pre, c, 8)) continue;
            const uint8_t *all = scan_ptr(im, fs, 0x200);
            for (size_t k = 0; all && k + 12 <= 0x200; k++)
                if (cmd_pmatch(all + k, 12, UC[u].any, c, 8)) { o->ctor = fs; return 1; }
        }
        if (s->ctor == CTOR_ARRAY &&
            cmd_pmatch(pre, 4, "55 48 89 e5", c, 8) &&
            cmd_pmatch(body, 0x60, "48 8d 05 D 48 89 07 4c 8b 26 4c 63 6e 0c 0f 57 c0 0f 11 47 28", c, 8)) {
            o->ctor = fs; return 1;
        }
    }
    return 0;
}

int scan_cmd_type(const scan_image *im, const post_layout *pl, const stage2_common *cm, const cmd_spec *s, cmd_type *o) {
    memset(o, 0, sizeof *o);
    o->spec = s;
    o->need = C_ALL_FIELDS | (s->build == BUILD_CTOR ? C_CTOR : 0);
    scan_cap f, vt, tid;
    if (!pl || !pl->ok || !cm || !cm->ok) { snprintf(o->evidence, sizeof o->evidence, "stage-2 common facts unresolved"); return 0; }
    if (scan_command_factory(im, s->cls, &f, &vt, &tid) || (f.ok && vt.ok)) {
        o->factory = f.value; o->vtable = vt.value; o->size = (uint32_t)f.extra; o->slots = (uint32_t)vt.extra;
        o->conf |= C_FACTORY;
        if (tid.ok) { o->type_id = (uint32_t)tid.value; o->conf |= C_TYPEID; }
        if (o->size == s->size) o->conf |= C_SIZE;
    }
    uint64_t gate_fn = 0;
    if ((o->conf & C_FACTORY) && scan_q(im, o->vtable + cm->valid_slot, &o->valid_fn) && in_text(im, o->valid_fn) &&
        scan_q(im, o->vtable + pl->auth_slot, &o->auth_fn) && in_text(im, o->auth_fn) &&
        scan_q(im, o->vtable + pl->auth_slot + 0x10, &o->write_fn) && in_text(im, o->write_fn) &&
        scan_q(im, o->vtable + pl->gate_slot, &gate_fn) && in_text(im, gate_fn) && o->slots * 8 > pl->auth_slot + 0x10) {
        /* a gameplay command: Post's "may be posted anywhere" slot is NOT constant true (that is the session class) */
        const uint8_t *g = scan_ptr(im, gate_fn, 8); uint32_t c[2];
        if (!(g && cmd_pmatch(g, 8, "55 48 89 e5 b0 01 5d c3", c, 2))) o->conf |= C_SLOTS;
    }
    if (s->auth_kind == AUTH_EVENT) {
        /* engineer 76: controls(cmd, &cmd[country], err) first, then the event handle's offset (lea r13, [r14 + H]); the
         * rest of the slot (cast, option count, pending record) is read by scan_events_facts */
        uint32_t c[6];
        const uint8_t *ab = (o->conf & C_SLOTS) ? scan_ptr(im, o->auth_fn, 0x40) : NULL;
        if (ab && cmd_pmatch(ab, 0x34, "55 48 89 e5 41 57 41 56 41 55 41 54 53 48 83 ec 18 48 89 f3 49 89 fe 4c 8d 67 B "
                                       "4c 89 e6 48 89 da e8 D 84 c0 0f 84 D 4d 8d 6e B", c, 6) && c[0] == s->auth_off) {
            o->controls_fn = o->auth_fn + 0x26 + (int64_t)(int32_t)c[1];
            o->target_hint = c[3]; /* the event handle's offset in the command */
            o->conf |= C_AUTH;
        }
        scan_cap vc;
        if ((o->conf & C_AUTH) && scan_class_vtable(im, s->obj_cast, &vc) && vc.value) { o->vt_obj = vc.value; o->conf |= C_RESOLVER; }
    } else if (s->auth_kind == AUTH_DIPLO) {
        /* engineer 78: the command's auth slot: if the action pointer at +A is set, controls(cmd, action + S, err):
         * mov rsi,[r14+A]; add rsi,S; mov rdi,r14; mov rdx,rbx; call controls */
        uint32_t c[6]; unsigned nn = 0;
        long p = (o->conf & C_SLOTS) ? pfind_at(im, o->auth_fn, 0x100, "49 8b 76 B 48 83 c6 B 4c 89 f7 48 89 da e8 D", c, 6, &nn) : -1;
        if (p >= 0 && c[0] == s->auth_off && c[1] < 0x40) {
            o->controls_fn = o->auth_fn + (uint64_t)p + 19 + (int64_t)(int32_t)c[2];
            o->act_sender_off = c[1];
            if (in_text(im, o->controls_fn)) o->conf |= C_AUTH;
        }
        /* the action class: exact vtable, its own constructor (the one taking the sender and receiver tags and the date,
         * calling the base constructor with (0, -1, 1) as every action of the diplomacy view does), its type id (the
         * SetType call that ends the constructor), its size (its own Clone allocates it) */
        scan_cap vc;
        if ((o->conf & C_AUTH) && scan_class_vtable(im, s->obj_cast, &vc) && vc.value) {
            o->vt_obj = vc.value;
            uint64_t st[16], clone = 0; unsigned ns = lea_sites(im, o->vt_obj, st, 16), found = 0;
            for (unsigned i = 0; i < ns && i < 16; i++) {
                uint64_t fs = fn_start(im, st[i]);
                const uint8_t *fb = fs ? scan_ptr(im, fs, 0xa0) : NULL;
                if (!fb) continue;
                int simple = s->ctor == CTOR_DIPLO_SIMPLE && cmd_pmatch(fb, 15, "55 48 89 e5 53 48 83 ec 28 48 89 fb 8b 41 08", c, 6);
                int war = s->ctor == CTOR_DIPLO_WAR &&
                          cmd_pmatch(fb, 26, "55 48 89 e5 41 57 41 56 53 48 83 ec 28 44 89 c3 49 89 cf 49 89 fe 41 8b 41 08", c, 6);
                if (!simple && !war) continue;
                unsigned m = 0;
                long b = pfind_at(im, fs, 0xa0, "c7 04 24 01 00 00 00 48 8d 4d B 45 31 c0 41 b9 ff ff ff ff e8 D 48 8d 05 D", c, 6, &m);
                if (b < 0 || m != 1) continue;
                uint64_t at = fs + (uint64_t)b, base = at + 25 + (int64_t)(int32_t)c[1], vt = at + 32 + (int64_t)(int32_t)c[2];
                if (vt != o->vt_obj || !in_text(im, base)) continue;
                long t2 = simple ? pfind_at(im, fs, 0xa0, "48 89 03 48 89 df be D", c + 3, 3, &m)
                                 : pfind_at(im, fs, 0xe0, "41 88 5e B 4c 89 f7 be D", c + 3, 3, &m);
                if (simple && t2 < 0) { /* improve relations: the constructor also sets one byte flag of its own first */
                    uint32_t e[3];
                    t2 = pfind_at(im, fs, 0xa0, "48 89 03 c6 43 B 01 48 89 df be D", e, 3, &m);
                    if (t2 >= 0) c[3] = e[1];
                }
                if (t2 < 0 || m != 1) continue;
                uint32_t typ = war ? c[4] : c[3], goal_off = 0, flag_off = war ? c[3] : 0;
                if (war) {
                    uint32_t g[2]; unsigned mg = 0;
                    if (pfind_at(im, fs, 0xe0, "49 8b 47 08 49 89 46 B", g, 2, &mg) < 0 || mg != 1) continue;
                    goal_off = g[0];
                }
                /* the complete- and base-object constructors are identical copies: every match must agree */
                if (found && (o->act_base != base || o->act_type != typ || o->act_goal_off != goal_off || o->act_flag_off != flag_off)) { found = 99; break; }
                if (!found) { o->act_ctor = fs; o->act_base = base; o->act_type = typ; o->act_goal_off = goal_off; o->act_flag_off = flag_off; }
                found = 1;
            }
            /* the class's own Clone (vtable +0xd0, the slot the command's copy constructor calls): new(size) first */
            if (found == 1 && scan_q(im, o->vt_obj + 0xd0, &clone) && in_text(im, clone)) {
                unsigned m = 0;
                long z = pfind_at(im, clone, 0x20, "bf D e8 D", c, 6, &m);
                if (z >= 0 && c[0] >= 0x6c && c[0] < 0x400) o->act_size = c[0];
            }
            if (found == 1 && o->act_size && o->act_type) { o->conf |= C_RESOLVER | C_CTOR; o->ctor = o->act_ctor; }
        }
    } else if (s->auth_kind == AUTH_TAG_PTR) {
        /* auth as AUTH_TAG; the object: a raw pointer of exactly the class obj_cast names (RTTI vtable) */
        uint32_t c[4];
        const uint8_t *ab = (o->conf & C_SLOTS) ? scan_ptr(im, o->auth_fn, 0x12) : NULL;
        if (ab && cmd_pmatch(ab, 0x11, "55 48 89 e5 48 89 f2 48 8d 77 B 5d e9 D", c, 4) && c[0] == s->auth_off) {
            o->controls_fn = o->auth_fn + 17 + (int64_t)(int32_t)c[1];
            o->conf |= C_AUTH;
        }
        scan_cap vc;
        if ((o->conf & C_AUTH) && scan_class_vtable(im, s->obj_cast, &vc) && vc.value) { o->vt_obj = vc.value; o->conf |= C_RESOLVER; }
    } else if (s->auth_kind == AUTH_TAG) {
        /* auth: mov rdx, rsi; lea rsi, [rdi + TAG]; pop rbp; jmp controls -- the sender must control the tag */
        uint32_t c[4];
        const uint8_t *ab = (o->conf & C_SLOTS) ? scan_ptr(im, o->auth_fn, 0x12) : NULL;
        if (ab && cmd_pmatch(ab, 0x11, "55 48 89 e5 48 89 f2 48 8d 77 B 5d e9 D", c, 4) && c[0] == s->auth_off) {
            o->controls_fn = o->auth_fn + 17 + (int64_t)(int32_t)c[1];
            o->conf |= C_AUTH;
        }
        /* the objects' class: the cast the "can do" check makes (CReferenceObject -> obj_cast) */
        const uint8_t *vb = (o->conf & C_AUTH) ? scan_ptr(im, o->valid_fn, 0x300) : NULL;
        for (size_t k = 0; vb && k + 14 <= 0x300 && !o->ti_dst; k++) {
            if (!cmd_pmatch(vb + k, 14, "48 8d 35 D 48 8d 15 D", c, 4)) continue;
            uint64_t a = o->valid_fn + k, src = a + 7 + (int64_t)(int32_t)c[0], dst = a + 14 + (int64_t)(int32_t)c[1];
            char sn[64], dn[64];
            if (ti_name(im, src, sn, sizeof sn) && !strcmp(sn, "16CReferenceObject") && ti_name(im, dst, dn, sizeof dn) &&
                !strcmp(dn, s->obj_cast)) { o->ti_src = src; o->ti_dst = dst; o->conf |= C_RESOLVER; }
        }
    } else {
        if ((o->conf & C_SLOTS) && auth_shape(im, o->auth_fn, s, o)) o->conf |= C_AUTH;
        if ((o->conf & C_AUTH) && resolver_shape(im, s, o)) o->conf |= C_RESOLVER;
    }
    /* engineer 75: a second object the "can do" casts (a leader, an army group): its typeinfo and hint from that cast */
    if (s->obj2 == OBJ2_CAST && (o->conf & C_RESOLVER)) {
        const uint8_t *vb = scan_ptr(im, o->valid_fn, 0x400);
        uint32_t c[4];
        for (size_t k = 0; vb && k + 30 <= 0x400 && !o->ti2; k++) {
            if (!cmd_pmatch(vb + k, 14, "48 8d 35 D 48 8d 15 D", c, 4)) continue;
            uint64_t a = o->valid_fn + k, src = a + 7 + (int64_t)(int32_t)c[0], dst = a + 14 + (int64_t)(int32_t)c[1];
            char sn[64], dn[64];
            if (!ti_name(im, src, sn, sizeof sn) || strcmp(sn, "16CReferenceObject") || !ti_name(im, dst, dn, sizeof dn) ||
                strcmp(dn, s->obj2_cast))
                continue;
            for (size_t e = 14; e + 5 <= 30; e++) { /* the hint: mov ecx, imm32 or xor ecx, ecx before the call */
                if (cmd_pmatch(vb + k + e, 2, "31 c9", c, 4)) { o->ti2 = dst; o->hint2 = 0; break; }
                if (cmd_pmatch(vb + k + e, 5, "b9 D", c, 4) && c[0] < 0x400) { o->ti2 = dst; o->hint2 = c[0]; break; }
            }
        }
        if (!o->ti2) o->conf &= ~(unsigned)C_RESOLVER;
    }
    if (o->conf & C_SLOTS) {
        int all = 1;
        for (int i = 0; i < s->nf; i++) {
            int ok1 = (s->f[i].kind == FK_REF_ARRAY || s->f[i].kind == FK_I32_ARRAY)
                          ? serial_has2(im, o->write_fn, s->f[i].field_id, s->f[i].count_off, s->f[i].off)
                          : serial_has(im, o->write_fn, s->f[i].field_id, s->f[i].off);
            if (!ok1) all = 0;
        }
        if (all) o->conf |= C_SERIAL;
    }
    if (s->build == BUILD_CTOR && s->auth_kind != AUTH_DIPLO && (o->conf & C_FACTORY) && find_ctor(im, s, o)) o->conf |= C_CTOR;
    o->ok = (o->conf & o->need) == o->need;
    snprintf(o->evidence, sizeof o->evidence,
             "%s: size 0x%x, type id %u, valid 0x%llx, auth 0x%llx (resolver 0x%llx -> %s +0x%x, owner 0x%llx), serializer "
             "0x%llx%s%s; confidence 0x%x of 0x%x",
             s->cls, o->size, o->type_id, (unsigned long long)o->valid_fn, (unsigned long long)o->auth_fn,
             (unsigned long long)o->resolve_fn, s->target, o->target_hint, (unsigned long long)o->owner_fn,
             (unsigned long long)o->write_fn, s->build == BUILD_CTOR ? ", ctor " : "",
             s->build == BUILD_CTOR ? (o->ctor ? "found" : "NOT found") : "", o->conf, o->need);
    return o->ok;
}

/* engineer 76: the event window and the pending-event records (see events_facts in cmd2.h). ev = the resolved
 * events.answer type (its ctor, auth and "can do" slots). Every fact comes from the code that uses it, and the facts
 * read from different functions must agree (the option count offset from the auth slot, the "can do" and the window's
 * update; the scope offset from the click and the update; the game-state global from stage 1). */
static int scan_events_facts(const scan_image *im, uint64_t gs_global, uint64_t generic_lookup, const cmd_type *ev, events_facts *o) {
    memset(o, 0, sizeof *o);
    char *e = o->evidence; size_t en = sizeof o->evidence;
    uint32_t c[8]; unsigned n;
    if (!ev || !(ev->conf & C_AUTH) || !ev->ctor || !ev->vt_obj) { snprintf(e, en, "answer type unresolved"); return 0; }
    o->vt_window = ev->vt_obj;
    o->ev_handle_off = ev->target_hint;
    o->cmd_country_off = ev->spec->auth_off;
    /* 1. the auth slot: the cast to CEvent, the option count, the pending lookup over the game state, the record's event */
    uint64_t a = ev->auth_fn;
    long k = pfind_at(im, a, 0x200, "48 8d 35 D 48 8d 15 D 48 89 c7 31 c9 e8", c, 8, &n);
    if (k < 0) { snprintf(e, en, "auth: no cast"); return 0; }
    {
        uint64_t at = a + (uint64_t)k, src = at + 7 + (int64_t)(int32_t)c[0], dst = at + 14 + (int64_t)(int32_t)c[1];
        char sn[64], dn[64];
        if (!ti_name(im, src, sn, sizeof sn) || strcmp(sn, "16CReferenceObject") || !ti_name(im, dst, dn, sizeof dn) ||
            strcmp(dn, ev->spec->target)) { snprintf(e, en, "auth: the cast is not CReferenceObject -> %s", ev->spec->target); return 0; }
        o->ti_reference = src; o->ti_event = dst;
    }
    /* every resolve the slot makes of the event handle (mov rdi, r13; call L) uses the generic lookup */
    unsigned calls = 0, other = 0;
    const uint8_t *ab = scan_ptr(im, a, 0x200);
    for (size_t i = 0; ab && i + 8 <= 0x200; i++)
        if (cmd_pmatch(ab + i, 8, "4c 89 ef e8 D", c, 8)) {
            if (a + i + 8 + (int64_t)(int32_t)c[0] == generic_lookup) calls++; else other++;
        }
    if (!calls || other) { snprintf(e, en, "auth: event resolves %u by the generic lookup, %u otherwise", calls, other); return 0; }
    if (pfind_at(im, a, 0x200, "83 b8 D 00 0f 84", c, 8, &n) < 0 || n != 1 || c[0] > 0x2000) { snprintf(e, en, "auth: option count check"); return 0; }
    o->ev_nopts_off = c[0];
    k = pfind_at(im, a, 0x280, "48 8d 05 D 48 8b 38 41 8b 76 B e8 D 48 85 c0", c, 8, &n);
    if (k < 0 || n != 1) { snprintf(e, en, "auth: pending lookup call"); return 0; }
    uint64_t at = a + (uint64_t)k;
    if (at + 7 + (int64_t)(int32_t)c[0] != gs_global) { snprintf(e, en, "auth: the pending lookup is not over the game state"); return 0; }
    o->cmd_id_off = c[1];
    uint64_t pend = at + 19 + (int64_t)(int32_t)c[2];
    const uint8_t *pb = in_text(im, pend) ? scan_ptr(im, pend, 0x20) : NULL;
    if (!pb || !cmd_pmatch(pb, 0x20, "55 48 89 e5 48 63 8f D 48 85 c9 74 ?? 48 8b 87 D 48 6b c9 B", c, 8) ||
        c[0] > 0x4000 || c[1] > 0x4000 || c[2] < 0x10 || c[2] > 0x100) { snprintf(e, en, "pending lookup body"); return 0; }
    o->pend_cnt_off = c[0]; o->pend_arr_off = c[1]; o->pend_size = c[2];
    if (pfind_at(im, a, 0x280, "48 3b 47 B 74", c, 8, &n) < 0 || n != 1 || c[0] + 8 > o->pend_size) { snprintf(e, en, "auth: record's event"); return 0; }
    o->pend_event_off = c[0];
    /* 2. the "can do": 0 <= option < count, the same count offset */
    if (pfind_at(im, ev->valid_fn, 0x300, "41 8b 86 D 85 c0 78 ?? 41 3b 84 24 D", c, 8, &n) < 0 || n != 1 || c[1] != o->ev_nopts_off) {
        snprintf(e, en, "can-do: option range"); return 0;
    }
    o->cmd_option_off = c[0];
    if (o->cmd_option_off != ev->spec->f[3].off || o->cmd_id_off != ev->spec->f[0].off || o->ev_handle_off != ev->spec->f[1].off) {
        snprintf(e, en, "code offsets differ from the serializer's"); return 0;
    }
    /* 3. the thunk right after the constructor (push rbp; mov rbp,rsp; pop rbp; jmp ctor) and its ONE caller: the window's
     *    option click, which loads every argument from the window and the option's index from the clicked option */
    const uint8_t *z = scan_ptr(im, ev->ctor, 0x800);
    for (size_t i = 0; z && i + 10 <= 0x800 && !o->thunk; i++)
        if (cmd_pmatch(z + i, 10, "55 48 89 e5 5d e9 D", c, 8) && ev->ctor + i + 10 + (int64_t)(int32_t)c[0] == ev->ctor) o->thunk = ev->ctor + i;
    if (!o->thunk) { snprintf(e, en, "constructor thunk"); return 0; }
    const uint8_t *tx = scan_ptr(im, im->text_addr, im->text_size);
    unsigned sites = 0, good = 0;
    for (uint64_t i = 0x80; tx && i + 5 <= im->text_size; i++) {
        if (tx[i] != 0xe8 || im->text_addr + i + 5 + (int64_t)(int32_t)rd32(tx + i + 1) != o->thunk) continue;
        sites++;
        /* mov esi,[rbx+ID]; mov rdx,[rbx+EV]; lea rcx,[rbx+CO]; lea r8,[rbx+SC]; mov r9d,[r12+OPT]; mov rdi,rax; call */
        if (!cmd_pmatch(tx + i - 26, 26, "8b 73 B 48 8b 53 B 48 8d 4b B 4c 8d 43 B 45 8b 8c 24 D 48 89 c7", c, 8)) continue;
        uint32_t w[5] = {c[0], c[1], c[2], c[3], c[4]};
        uint32_t gate = 0;
        for (uint64_t b = i - 0x80; b < i - 26 && !gate; b++)
            if (cmd_pmatch(tx + b, 7, "80 bb D 00", c, 8) && c[0] < 0x2000) gate = c[0];
        if (!gate) continue;
        if (good && (w[0] != o->win_id_off || w[1] != o->win_event_off || w[2] != o->win_country_off || w[3] != o->win_scope_off ||
                     w[4] != o->opt_index_off || gate != o->win_open_off)) { snprintf(e, en, "option clicks disagree"); return 0; }
        o->win_id_off = w[0]; o->win_event_off = w[1]; o->win_country_off = w[2]; o->win_scope_off = w[3]; o->opt_index_off = w[4];
        o->win_open_off = gate; good++;
    }
    if (good < 1 || sites > 8) { snprintf(e, en, "constructor thunk: %u callers, %u option clicks", sites, good); return 0; }
    /* 4. the window's per-frame update, a virtual of CEventWindow: the event's options, the window flag, the option test
     *    called with the window's scope (the click's scope offset) for each option */
    for (int sl = 0; sl < 80 && !o->avail_fn; sl++) {
        uint64_t f = 0;
        if (!scan_q(im, o->vt_window + 8 * (uint64_t)sl, &f) || !in_text(im, f)) continue;
        long r = pfind_at(im, f, 0x800, "4c 63 a8 D 4d 85 ed 74 ?? 4c 8b b0 D 49 83 c4 B 49 c1 e5 03 31 db 4d 8b 3c 1e "
                                        "48 8b 45 ?? 0f b6 90 D 4c 89 ff 4c 89 e6 e8 D 84 c0", c, 8, &n);
        if (r < 0 || n != 1) continue;
        if (c[0] != o->ev_nopts_off || c[2] != o->win_scope_off || c[3] > 0x4000 || c[1] > 0x2000) { snprintf(e, en, "window update disagrees"); return 0; }
        o->ev_opts_off = c[1]; o->win_flag_off = c[3];
        o->avail_fn = f + (uint64_t)r + 55 + (int64_t)(int32_t)c[4];
        if (!in_text(im, o->avail_fn)) o->avail_fn = 0;
    }
    if (!o->avail_fn) { snprintf(e, en, "window update (option test) not found in CEventWindow's vtable"); return 0; }
    o->ok = 1;
    snprintf(e, en, "window id +0x%x event +0x%x country +0x%x scope +0x%x open +0x%x flag +0x%x; options +0x%x/+0x%x index "
             "+0x%x; pending gs+0x%x/+0x%x size 0x%x event +0x%x; test 0x%llx",
             o->win_id_off, o->win_event_off, o->win_country_off, o->win_scope_off, o->win_open_off, o->win_flag_off,
             o->ev_opts_off, o->ev_nopts_off, o->opt_index_off, o->pend_arr_off, o->pend_cnt_off, o->pend_size,
             o->pend_event_off, (unsigned long long)o->avail_fn);
    return 1;
}


/* engineer 78: the facts every diplomatic action needs (see diplo_facts in cmd2.h), each from the code that uses it */
static void scan_diplo_facts(const scan_image *im, const stage2_table *t, diplo_facts *o) {
    memset(o, 0, sizeof *o);
    const cmd_type *any = NULL, *war = NULL;
    for (int i = 0; i < t->n; i++) {
        if (t->t[i].spec->auth_kind != AUTH_DIPLO || !t->t[i].act_ctor) continue;
        if (!any) any = &t->t[i];
        if (t->t[i].spec->ctor == CTOR_DIPLO_WAR) war = &t->t[i];
    }
    if (!any) { snprintf(o->evidence, sizeof o->evidence, "no diplomatic action resolved"); return; }
    uint32_t c[6]; unsigned m = 0;
    /* the command maker: mov rax,[rax]; mov eax,[rax+DATE]; mov [r12+ADATE],eax; mov rdi,rbx; mov rsi,r14; call make_unique,
     * where make_unique is the one function that stores the command's vtable into a fresh 0x30 object */
    long p = pfind_at(im, im->text_addr, im->text_size, "48 8b 00 8b 80 D 41 89 44 24 B 48 89 df 4c 89 f6 e8 D", c, 6, &m);
    if (p < 0 || m != 1) { snprintf(o->evidence, sizeof o->evidence, "command maker: %u matches", m); return; }
    uint64_t at = im->text_addr + (uint64_t)p;
    o->date_off = c[0]; o->act_date_off = c[1];
    o->make_unique = at + 25 + (int64_t)(int32_t)c[2];
    o->make_cmd = fn_start(im, at);
    uint64_t st[32]; unsigned ns = lea_sites(im, any->vtable, st, 32), hit = 0;
    for (unsigned i = 0; i < ns && i < 32; i++) if (fn_start(im, st[i]) == o->make_unique) hit++;
    if (!hit || !o->make_cmd || o->date_off < 0x100 || o->date_off > 0x2000 || o->act_date_off >= 0x6c) {
        snprintf(o->evidence, sizeof o->evidence, "command maker 0x%llx: its helper does not build the command", (unsigned long long)o->make_cmd);
        return;
    }
    /* the date object the constructors build: lea rax,[rip+G]; lea rcx,[rax+0x10]; mov [rbp-X],rcx; add rax,0x60 */
    long d = pfind_at(im, any->act_ctor, 0x60, "48 8d 05 D 48 8d 48 10 48 89 4d B 48 83 c0 60", c, 6, &m);
    if (d < 0) { snprintf(o->evidence, sizeof o->evidence, "date layout not found in the action constructor"); return; }
    o->date_vt = any->act_ctor + (uint64_t)d + 7 + (int64_t)(int32_t)c[0];
    o->ok = 1;
    /* the player's war goals as the declare-war dialog lists them: add rdi,0x10 (the action's sender); call country_of;
     * mov rbx,[rax+HOLDER]; mov eax,[rbx+COUNT] ... mov rax,[rax+HOLDER]; mov [rbp-X],rax; mov r15,[rbx+LIST] ... per goal
     * mov rdi,[r15]; mov eax,[rdi+TARGET]; and the handler writes the chosen goal's handle to the war action's field */
    if (war) {
        const uint8_t *tx = scan_ptr(im, im->text_addr, im->text_size);
        unsigned votes = 0;
        for (uint64_t i = 0; tx && i + 0x200 < im->text_size; i++) {
            if (tx[i] != 0x48 || !cmd_pmatch(tx + i, 22, "48 83 c7 10 e8 D 48 8b 98 D 8b 43 B", c, 6)) continue;
            uint64_t a = im->text_addr + i, cof = a + 9 + (int64_t)(int32_t)c[0];
            uint32_t holder = c[1], cnt = c[2], q[4]; unsigned mm = 0;
            long l = pfind_at(im, a, 0x100, "48 8b 80 D 48 89 45 B 4c 8b 7b B", q, 4, &mm);
            if (l < 0 || q[0] != holder) continue;
            uint32_t list = q[2];
            long g = pfind_at(im, a + (uint64_t)l, 0x100, "49 8b 3f 8b 47 B", q, 4, &mm);
            if (g < 0) continue;
            uint32_t tgt = q[0];
            uint64_t fs = fn_start(im, a);
            long w = fs ? pfind_at(im, fs, 0x800, "49 89 47 B", q, 4, &mm) : -1;
            int writes = 0;
            for (long k = w; fs && k >= 0 && k < 0x800; ) { /* any "mov [r15+GOAL],rax" in the handler */
                const uint8_t *b2 = scan_ptr(im, fs + (uint64_t)k, 4);
                if (b2 && b2[3] == war->act_goal_off) { writes = 1; break; }
                long nx = pfind_at(im, fs + (uint64_t)k + 1, 0x800 - (size_t)k - 1, "49 89 47 B", q, 4, &mm);
                k = nx < 0 ? -1 : k + 1 + nx;
            }
            if (!writes || !in_text(im, cof)) continue;
            if (!votes) { o->country_of = cof; o->wg_holder_off = holder; o->wg_count_off = cnt; o->wg_list_off = list; o->wg_target_off = tgt; }
            else if (o->country_of != cof || o->wg_holder_off != holder || o->wg_list_off != list || o->wg_target_off != tgt) { votes = 99; break; }
            votes++;
        }
        o->war_ok = votes >= 1 && votes < 99 && o->wg_list_off < o->wg_count_off && o->wg_target_off < 0x200;
    }
    snprintf(o->evidence, sizeof o->evidence, "make_cmd 0x%llx (helper 0x%llx), date +0x%x -> action +0x%x, date vt 0x%llx; war goals %s: country_of 0x%llx holder +0x%x list +0x%x count +0x%x target +0x%x",
             (unsigned long long)o->make_cmd, (unsigned long long)o->make_unique, o->date_off, o->act_date_off,
             (unsigned long long)o->date_vt, o->war_ok ? "ok" : "NOT found", (unsigned long long)o->country_of,
             o->wg_holder_off, o->wg_list_off, o->wg_count_off, o->wg_target_off);
}

/* engineer 79: the area-defence order facts (see holds_facts in cmd2.h), from the state click's own execution (vtable
 * +0x58): mov rdi,r14; mov esi,KIND; call create  /  mov eax,[r14+CNT]; test eax,eax; jle; mov rcx,[r14+ARR]  /
 * mov esi,[rbx+0x4c]; mov rdi,r15; call SET where SET = push rbp; mov rbp,rsp; mov [rdi+OFF],esi; pop rbp; ret */
static void scan_holds_facts(const scan_image *im, const cmd_type *ad, holds_facts *o) {
    memset(o, 0, sizeof *o);
    uint64_t ex = 0; uint32_t c[4]; unsigned m = 0;
    if (!ad || !(ad->conf & C_FACTORY) || !scan_q(im, ad->vtable + 0x58, &ex) || !in_text(im, ex)) { snprintf(o->evidence, sizeof o->evidence, "no execution slot"); return; }
    long k = pfind_at(im, ex, 0x400, "4c 89 f7 be D e8 D", c, 4, &m);
    if (k >= 0 && c[0] >= 1 && c[0] < 32) o->kind_area = c[0];
    long a = pfind_at(im, ex, 0x400, "41 8b 86 D 85 c0 7e ?? 49 8b 8e D", c, 4, &m);
    if (a >= 0 && c[0] < 0x400 && c[1] < 0x400 && c[1] < c[0]) { o->area_cnt = c[0]; o->area_off = c[1]; }
    long st = pfind_at(im, ex, 0x400, "8b 73 4c 4c 89 ff e8 D", c, 4, &m);
    if (st >= 0) {
        uint64_t f = ex + (uint64_t)st + 11 + (int64_t)(int32_t)c[0];
        const uint8_t *fb = in_text(im, f) ? scan_ptr(im, f, 12) : NULL;
        if (fb && cmd_pmatch(fb, 12, "55 48 89 e5 89 b7 D 5d c3", c, 4) && c[0] < 0x400) o->setting_off = c[0];
    }
    o->ok = o->kind_area && o->area_off && o->area_cnt && o->setting_off;
    snprintf(o->evidence, sizeof o->evidence, "exec 0x%llx: area kind %u, states +0x%x count +0x%x, settings +0x%x",
             (unsigned long long)ex, o->kind_area, o->area_off, o->area_cnt, o->setting_off);
}

/* engineer 82: the building reference, from the game's own reference constructors (template rsi, location rdx): the
 * template's "real" byte and its province count decide which constructor applies; the state one stores the state's id,
 * the province one the province's id; both store the template's id at +0xc and the location at +8. Every match on the
 * build must agree. The vptr the command's constructor stores at +0x28 must be CBuildingReference's own vtable. */
static void scan_construction_facts(const scan_image *im, const cmd_type *add, construction_facts *o) {
    memset(o, 0, sizeof *o);
    const uint8_t *tx = scan_ptr(im, im->text_addr, im->text_size);
    uint32_t c[8]; unsigned ns = 0, np = 0, bad = 0;
    for (uint64_t i = 0; tx && i + 40 <= im->text_size; i++) {
        if (tx[i] != 0x80 || tx[i + 1] != 0x7e) continue;
        int st = cmd_pmatch(tx + i, 27, "80 7e B 00 74 ?? 83 be D 00 7f ?? 8b 46 B 89 47 0c 8b 42 B 89 47 08", c, 8);
        int pv = !st && cmd_pmatch(tx + i, 30, "80 7e B 00 74 ?? 83 be D 00 7e ?? 8b 46 B 89 47 0c 8b 82 D 89 47 08", c, 8);
        if (!st && !pv) continue;
        if (!o->tmpl_real_off) { o->tmpl_real_off = c[0]; o->tmpl_prov_off = c[1]; o->tmpl_id_off = c[2]; }
        if (c[0] != o->tmpl_real_off || c[1] != o->tmpl_prov_off || c[2] != o->tmpl_id_off) { bad++; continue; }
        if (st) { if (o->state_id_off && o->state_id_off != c[3]) bad++; o->state_id_off = c[3]; ns++; }
        else { if (o->prov_id_off && o->prov_id_off != c[3]) bad++; o->prov_id_off = c[3]; np++; }
    }
    scan_cap vc;
    uint64_t cls = scan_class_vtable(im, "18CBuildingReference", &vc) ? vc.value : 0, stored = 0;
    if (add && add->ctor) {
        unsigned m = 0;
        long k = pfind_at(im, add->ctor, 0x60, "48 8d 05 D 48 83 c0 10 48 89 47 28", c, 8, &m);
        if (k >= 0 && m == 1) stored = add->ctor + (uint64_t)k + 7 + (int64_t)(int32_t)c[0] + 0x10;
    }
    o->ref_vt = stored == cls ? cls : 0;
    o->ok = !bad && ns >= 1 && np >= 1 && o->tmpl_real_off && o->tmpl_prov_off && o->tmpl_id_off && o->ref_vt;
    snprintf(o->evidence, sizeof o->evidence, "%u state / %u province reference ctors (%u disagree): template real +0x%x, "
             "province +0x%x, id +0x%x; state id +0x%x, province id +0x%x; reference vtable 0x%llx (ctor stores 0x%llx)",
             ns, np, bad, o->tmpl_real_off, o->tmpl_prov_off, o->tmpl_id_off, o->state_id_off, o->prov_id_off,
             (unsigned long long)cls, (unsigned long long)stored);
}

int scan_stage2(const scan_image *im, const post_layout *pl, uint64_t gs_global, stage2_table *t) {
    memset(t, 0, sizeof *t);
    scan_stage2_common(im, pl, gs_global, &t->common);
    scan_orders_facts(im, &t->orders);
    scan_fronts_facts(im, &t->fronts);
    int on = 0;
    for (int i = 0; i < CMD2_NSPECS && i < CMD2_MAX; i++) {
        on += scan_cmd_type(im, pl, &t->common, &CMD2_SPECS[i], &t->t[i]);
        t->n++;
    }
    /* AUTH_TAG types: the generic lookup (for the module's own object check) is the one every handle type uses */
    for (int i = 1; i < t->n; i++)
        if (t->t[i].spec->auth_kind != AUTH_HANDLE && t->t[0].ok) t->t[i].lookup_fn = t->t[0].lookup_fn;
    /* engineer 76: the event window and pending records (the answer type's own slots, the generic lookup) */
    {
        const cmd_type *ev = NULL;
        for (int i = 0; i < t->n; i++) if (t->t[i].spec->auth_kind == AUTH_EVENT) ev = &t->t[i];
        if (ev && t->t[0].ok) scan_events_facts(im, gs_global, t->t[0].lookup_fn, ev, &t->events);
    }
    scan_diplo_facts(im, t, &t->diplo); /* engineer 78 */
    scan_holds_facts(im, stage2_find(t, "holds.area_defense"), &t->holds); /* engineer 79 */
    scan_construction_facts(im, stage2_find(t, "construction.add"), &t->build); /* engineer 82 */
    /* every type must agree on the "sender controls" check and the generic lookup (one game, one rule) */
    for (int i = 1; i < t->n; i++)
        if (t->t[i].ok && t->t[0].ok &&
            (t->t[i].controls_fn != t->t[0].controls_fn || t->t[i].lookup_fn != t->t[0].lookup_fn)) {
            t->t[i].ok = 0; on--;
            snprintf(t->t[i].evidence + strlen(t->t[i].evidence), sizeof t->t[i].evidence - strlen(t->t[i].evidence),
                     "; disagrees with the other types' authorisation helpers");
        }
    /* engineer 75: the army-structure facts, from the code that uses them.
     * (1) a division's theatre as the new-army card reads it: the new-army constructor's C1 thunk (push rbp; mov rbp,
     *     rsp; pop rbp; jmp ctor, right after it) is called by the UI right after "call G" where G is a one-load getter
     *     (push rbp; mov rbp,rsp; mov rax,[rdi+OFF]; pop rbp; ret) on the first selected division; every such site agrees.
     * (2) an orders group's parent group, as COrderAssignCommand's execution walks it when the named order is not the
     *     group's own: mov rdi,[r14+OFF]; test rdi,rdi; je; mov esi,[rbx+0x48] (the order id); call lookup */
    {
        const cmd_type *na = NULL, *as = NULL;
        for (int i = 0; i < t->n; i++) {
            if (!strcmp(t->t[i].spec->op, "units.new_army") && t->t[i].ctor) na = &t->t[i];
            if (!strcmp(t->t[i].spec->op, "units.transfer") && (t->t[i].conf & C_FACTORY)) as = &t->t[i];
        }
        uint32_t c[4];
        if (na) {
            const uint8_t *z = scan_ptr(im, na->ctor, 0x800);
            uint64_t thunk = 0;
            for (size_t k = 0; z && k + 10 <= 0x800 && !thunk; k++)
                if (cmd_pmatch(z + k, 10, "55 48 89 e5 5d e9 D", c, 4) && na->ctor + k + 10 + (int64_t)(int32_t)c[0] == na->ctor)
                    thunk = na->ctor + k;
            const uint8_t *tx = scan_ptr(im, im->text_addr, im->text_size);
            unsigned votes = 0, other = 0; uint32_t off = 0;
            for (uint64_t i = 0x20; thunk && tx && i + 5 <= im->text_size; i++) {
                if (tx[i] != 0xe8 || im->text_addr + i + 5 + (int64_t)(int32_t)rd32(tx + i + 1) != thunk) continue;
                for (uint64_t b = i - 0x20; b < i; b++) {
                    if (tx[b] != 0xe8) continue;
                    uint64_t g = im->text_addr + b + 5 + (int64_t)(int32_t)rd32(tx + b + 1);
                    const uint8_t *gb = in_text(im, g) ? scan_ptr(im, g, 13) : NULL;
                    if (gb && cmd_pmatch(gb, 13, "55 48 89 e5 48 8b 87 D 5d c3", c, 4) && c[0] < 0x2000) {
                        if (!off) off = c[0];
                        if (c[0] == off) votes++; else other++;
                    }
                }
            }
            if (votes >= 1 && !other) t->common.unit_theatre_off = off;
        }
        if (as) {
            uint64_t ex = 0;
            if (scan_q(im, as->vtable + 0x58, &ex) && in_text(im, ex)) {
                unsigned n2 = 0;
                long r = pfind_at(im, ex, 0x100, "49 8b be D 48 85 ff 74 ?? 8b 73 48 e8", c, 4, &n2);
                if (r >= 0 && n2 == 1 && c[0] < 0x1000) t->common.group_parent_ptr_off = c[0];
            }
        }
    }
    /* family facts the per-family checks and read-backs need */
    for (int i = 0; i < t->n; i++) {
        const char *fam = t->t[i].spec->family;
        if (!strcmp(fam, "production") && !t->common.line_factories_off && t->t[i].ok) { t->t[i].ok = 0; on--; }
        if ((!strcmp(t->t[i].spec->op, "air.enable_mission") || t->t[i].spec->obj2 == OBJ2_REGION_ID) && !t->common.region_id_off && t->t[i].ok) { t->t[i].ok = 0; on--; }
        if (t->t[i].spec->obj2 == OBJ2_REGION_ID && (!t->common.regions_arr_off || !t->common.regions_cnt_off) && t->t[i].ok) { t->t[i].ok = 0; on--; }
        if (!strcmp(fam, "orders") && !t->orders.ok && t->t[i].ok) { t->t[i].ok = 0; on--; }
        /* orders2: the order tree, the allocator (array fields), the division facts; new_root also the fronts */
        if (!strcmp(fam, "orders2") && t->t[i].ok &&
            (!t->orders.ok || !t->common.alloc_slot || !t->common.ti_unit || !t->common.unit_group_off ||
             (t->t[i].spec->obj2 == OBJ2_FRONT && !t->fronts.ok))) { t->t[i].ok = 0; on--; }
        /* units: the division facts, the order tree (the transfer's order), the parent walk, the theatre read */
        if (!strcmp(fam, "units") && t->t[i].ok &&
            (!t->orders.ok || !t->fronts.ok || !t->common.alloc_slot || !t->common.ti_unit || !t->common.unit_group_off ||
             !t->common.unit_theatre_off || !t->common.group_parent_ptr_off || !t->common.ti_reference)) { t->t[i].ok = 0; on--; }
        if (!strcmp(fam, "events") && !t->events.ok && t->t[i].ok) { t->t[i].ok = 0; on--; }
        /* holds: the area order's facts, the order tree (the order named by id), the array allocator is the ctor's own */
        if (!strcmp(fam, "holds") && t->t[i].ok && (!t->holds.ok || !t->orders.ok)) { t->t[i].ok = 0; on--; }
        /* construction: the building reference (add) */
        if (!strcmp(t->t[i].spec->op, "construction.add") && t->t[i].ok && !t->build.ok) { t->t[i].ok = 0; on--; }
        /* diplomacy: the command maker and the date; declare war also the war-goal list */
        if (t->t[i].spec->auth_kind == AUTH_DIPLO && t->t[i].ok &&
            (!t->diplo.ok || (t->t[i].spec->ctor == CTOR_DIPLO_WAR && !t->diplo.war_ok))) { t->t[i].ok = 0; on--; }
        if (t->t[i].spec->obj2 == OBJ2_ORDERS_GROUP && (!t->common.ti_reference || !t->common.ti_orders_group) && t->t[i].ok) { t->t[i].ok = 0; on--; }
    }
    return on;
}

const cmd_type *stage2_find(const stage2_table *t, const char *op) {
    for (int i = 0; i < t->n; i++) if (!strcmp(t->t[i].spec->op, op)) return &t->t[i];
    return NULL;
}

int stage2_family_on(const stage2_table *t, const char *family) {
    int any = 0;
    if (!t->common.ok) return 0;
    for (int i = 0; i < t->n; i++)
        if (!strcmp(t->t[i].spec->family, family)) { if (!t->t[i].ok) return 0; any = 1; }
    return any;
}

const char *stage2_json(const stage2_table *t, char *buf, size_t n) {
    const stage2_common *c = &t->common;
    size_t at = (size_t)snprintf(buf, n,
                                 "{\"common\":{\"ok\":%s,\"valid_slot\":%u,\"delete_slot\":%u,\"exec_flag_off\":%u,"
                                 "\"allow_flag_off\":%u,\"player_tag_off\":%u,\"gate_flag_off\":%u,\"null_ref\":\"0x%llx\","
                                 "\"handle_off\":%u,\"line_factories_off\":%u,\"region_id_off\":%u,\"regions_arr_off\":%u,"
                                 "\"regions_cnt_off\":%u,\"ti_orders_group\":\"0x%llx\",\"alloc_slot\":%u,\"ti_unit\":\"0x%llx\","
                                 "\"unit_hint\":%u,\"unit_group_off\":%u,\"unit_theatre_off\":%u,\"group_parent_ptr_off\":%u},\"types\":[",
                                 c->ok ? "true" : "false", c->valid_slot, c->delete_slot, c->exec_flag_off, c->allow_flag_off,
                                 c->player_tag_off, c->gate_flag_off, (unsigned long long)c->null_ref_global,
                                 c->cref_handle_off, c->line_factories_off, c->region_id_off, c->regions_arr_off,
                                 c->regions_cnt_off, (unsigned long long)c->ti_orders_group, c->alloc_slot,
                                 (unsigned long long)c->ti_unit, c->unit_hint, c->unit_group_off, c->unit_theatre_off,
                                 c->group_parent_ptr_off);
    for (int i = 0; i < t->n && at < n; i++) {
        const cmd_type *x = &t->t[i];
        at += (size_t)snprintf(buf + at, n - at,
                               "%s{\"op\":\"%s\",\"ok\":%s,\"type_id\":%u,\"size\":%u,\"conf\":%u,\"need\":%u,\"factory\":\"0x%llx\","
                               "\"vtable\":\"0x%llx\",\"valid\":\"0x%llx\",\"resolver\":\"0x%llx\",\"owner\":\"0x%llx\","
                               "\"hint\":%u,\"ctor\":\"0x%llx\"}",
                               i ? "," : "", x->spec->op, x->ok ? "true" : "false", x->type_id, x->size, x->conf, x->need,
                               (unsigned long long)x->factory, (unsigned long long)x->vtable, (unsigned long long)x->valid_fn,
                               (unsigned long long)x->resolve_fn, (unsigned long long)x->owner_fn, x->target_hint,
                               (unsigned long long)x->ctor);
    }
    if (at < n) at += (size_t)snprintf(buf + at, n - at, "],\"orders\":");
    if (at < n) { orders_facts_json(&t->orders, buf + at, n - at); at += strlen(buf + at); }
    if (at < n) at += (size_t)snprintf(buf + at, n - at, ",\"fronts\":");
    if (at < n) { fronts_facts_json(&t->fronts, buf + at, n - at); at += strlen(buf + at); }
    if (at < n) {
        const events_facts *e = &t->events;
        at += (size_t)snprintf(buf + at, n - at,
                               ",\"events\":{\"ok\":%s,\"vt_window\":\"0x%llx\",\"avail\":\"0x%llx\",\"thunk\":\"0x%llx\",\"win\":[%u,%u,%u,%u,%u,%u],"
                               "\"event\":[%u,%u,%u],\"option_index\":%u,\"pending\":[%u,%u,%u,%u],\"cmd\":[%u,%u,%u],\"evidence\":\"%s\"}",
                               e->ok ? "true" : "false", (unsigned long long)e->vt_window, (unsigned long long)e->avail_fn,
                               (unsigned long long)e->thunk, e->win_id_off, e->win_event_off, e->win_country_off, e->win_scope_off,
                               e->win_open_off, e->win_flag_off, e->ev_handle_off, e->ev_opts_off, e->ev_nopts_off, e->opt_index_off,
                               e->pend_arr_off, e->pend_cnt_off, e->pend_size, e->pend_event_off, e->cmd_id_off,
                               e->cmd_country_off, e->cmd_option_off, e->evidence);
    }
    if (at < n) snprintf(buf + at, n - at, "}");
    return buf;
}
