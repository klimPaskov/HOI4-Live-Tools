/* HOI4 Live Tools in-process module: STAGE 2, the generic command builder and the command families.
 *
 * Pure computation over an image view, like scan.c and cmd.c. Stage 1 proved ONE command (game speed). Stage 2 posts
 * the commands that carry real play, the way the game's own UI posts them. Read from the binary (1.19.4.1, the same on
 * 1.20.0.0):
 *   - The UI's post helpers do: cmd = new T; T's constructor (or the factory plus the fields); ok = cmd->vfunc(VALID)(cmd, 0);
 *     ok ? Post(session, cmd, 0) : cmd->vfunc(8)(cmd) (the deleting destructor). VALID is the game's own "can do"
 *     check (the one that greys the UI's buttons); the module calls it before every post and refuses when it says no.
 *     The slot is read from the game's AI post helper (the function that logs "AI tried to post an invalid command"),
 *     which makes the same call, so it is never assumed.
 *   - Every gameplay command references game objects by an 8-byte handle (CRef: {type, id}) that a game resolver maps
 *     back to the object. Each command's authorisation slot (the one Post asks: "sender not authorized") has one
 *     shape: resolve the handle with a typed resolver, take the object's owner tag with an owner getter, ask whether
 *     the sender controls that country. The scan reads the typed resolver, the owner getter and the handle offsets
 *     from that slot, so the module can check BEFORE posting, with the game's own functions, that each object the
 *     client named is a live object of the right class (resolve(handle(obj)) == obj) and belongs to the player's
 *     country (owner(obj) == the player tag, the field the game's own per-frame code compares to set the session's
 *     command gate).
 *   - Post drops a gameplay command (its "may be posted anywhere" slot is false) when the session gate byte is clear
 *     (no country played: observer) or while the session is executing commands. The module's job runs on the main
 *     thread inside the Cocoa event pump, outside command execution (the session's executor runs from the idler's
 *     frame update on the same thread); it still reads both bytes, found in Post's own code, and refuses if either
 *     says Post would drop.
 * A command type is ON only when every check of its spec resolved on the running build (its "confidence" is a bitmask
 * of the checks; all must pass); otherwise that type stays off and the GUI path serves it. */
#ifndef HOI4_INPROC_CMD2_H
#define HOI4_INPROC_CMD2_H
#include "cmd.h"
#include "orders.h"

enum { FK_I32 = 1, FK_U8, FK_REF, FK_REF_ARRAY, FK_I32_ARRAY /* a CPdxArray<int> (province ids) */,
       /* engineer 74 (24 Nov): a 64-bit scalar (a fixed-point range end) and a raw handle the module took from a checked
        * object of another class (a CFront's own handle: the front a plan root is bound to) */
       FK_I64, FK_HANDLE,
       /* engineer 75 (24 Nov): a libc++ std::string held inline (the UI constructor copies a short string; names <= 22 bytes) */
       FK_STR,
       /* engineer 75 (picks): the player's own country tag; a raw object pointer (a focus: no handle, an exact vtable) */
       FK_TAG, FK_PTR,
       /* engineer 85: the second object as a raw pointer (OBJ2_PTR_EXACT: a doctrine folder template) */
       FK_PTR2 };
/* How the module builds the command:
 *   BUILD_FIELDS: the game's factory makes the default object; the module writes the spec's scalar and handle fields
 *                 into that fresh object (each field's default and serializer entry checked first).
 *   BUILD_CTOR:   payloads with containers: the module calls the UI's own constructor of the type (found by scan, the
 *                 one whose argument stores hit the serializer's offsets) on the factory's fresh object, so the game's
 *                 code allocates and fills its own containers. */
enum { BUILD_FIELDS = 1, BUILD_CTOR };
enum { CTOR_NONE = 0, CTOR_WING_I32_U8 = 1, CTOR_WING_I32_NULL = 2,
       /* engineer 71 (24 Nov): the rest of the air family, each the UI's own constructor read from its argument stores */
       CTOR_OBJ = 3,          /* (cmd, obj): one handle appended to the list (detach a wing from its army) */
       CTOR_TAGP_OBJ = 4,     /* (cmd, const tag *, obj): the country tag then one handle (disband a wing) */
       CTOR_OBJ_OBJ2 = 5,     /* (cmd, obj, obj2): one handle in the list, obj2's handle in its own field (attach) */
       CTOR_ARRAY = 6,        /* (cmd, const CPdxArray<T *> *): ONE command for every object (merge wings) */
       CTOR_ADDLINE = 7,      /* (cmd, const tag *, variant, organisation or NULL, at_top): a new production line */
       /* engineer 73 (24 Nov): battle plans */
       CTOR_ORDER_PATH = 8,   /* (cmd, COrderInstance *, const CPdxArray<int> *provinces, bool ui): a new path for an order */
       CTOR_NEW_FRONT = 9,    /* (cmd, group, const CPdxArray<int> *provinces, parent order, const CPdxArray<CArmy *> *
                               *  divisions, bool, bool blitz, bool withdraw, drawing group): a new front under a parent */
       /* engineer 75 (24 Nov): army structure (family `units`), each the UI's own constructor read from its argument stores */
       CTOR_DIVS_THEATRE = 10, /* (cmd, const CPdxArray<CUnit *> *, theatre, theater group NULL): a new army of these divisions */
       CTOR_LEADER = 11,       /* (cmd, CArmyLeader * or NULL, group, 0): a general / field marshal for an army / army group */
       CTOR_GROUP_NAME = 12,   /* (cmd, group, const std::string *): rename an army or army group */
       CTOR_GROUPS_OBJ = 13,   /* (cmd, const CPdxArray<COrdersGroup *> *, theatre): a new army group of these armies */
       CTOR_GROUPS_OBJ_I32 = 14, /* (cmd, const CPdxArray<COrdersGroup *> *, army group, -1): armies into an army group */
       CTOR_GROUPS = 15,       /* (cmd, const CPdxArray<COrdersGroup *> *): armies out of their army group */
       /* engineer 76 (24 Nov): an event answer, the event window's own call (cmd, pending id, CEvent *, const int *country,
        * const scope *, option index): the scope is copied by the game's own copy routine */
       CTOR_EVENT_OPTION = 16,
       /* engineer 78 (24 Nov): diplomacy. The command carries a CDiplomaticAction the module builds with the ACTION
        * class's own constructor, then wraps with the game's own command maker (it stamps the game date and moves the
        * action into a CDiplomaticActionCommand):
        *   SIMPLE (action, const tag *sender, const tag *receiver, const CDate *): guarantee, access, pact, relations;
        *   WAR    (action, const tag *sender, const tag *receiver, war goal object, flag byte, const CDate *) */
       CTOR_DIPLO_SIMPLE = 17, CTOR_DIPLO_WAR = 18,
       /* engineer 79 (24 Nov): an area-defence state click, the map tool's own constructor (cmd, group, order id or 0 =
        * a new area-defence order, state id, the order's settings mask): ONE state toggled into the order */
       CTOR_AREA_STATE = 19,
       /* engineer 79: research in a slot, the research view's own constructor (cmd, const tag *, technology template,
        * slot, XP boost byte, b35, UI byte on the stack) */
       CTOR_RESEARCH = 20,
       /* engineer 79: laws (ideas): the politics view's own constructors (cmd, const tag *, idea) and
        * (cmd, const tag *, the slot's current idea, the new idea) */
       CTOR_IDEA_ADD = 21, CTOR_IDEA_REPLACE = 22,
       /* engineer 82: construction: the map click's own constructor (cmd, mode byte, const tag *, const CBuildingReference *,
        * count); the reference {vptr, location id, building id, tag} is built by the module the way the game's reference
        * constructors build it (a state id for a state building, a province id for a province building) */
       CTOR_CONSTRUCTION = 23,
       /* engineer 105: a division's move (CMoveCommand): (cmd, CUnit *, province id, normal 1, kind 2, 1, four stack bytes
        * 0), the constants of the right click's own call for a reachable province (the province helper of the division's
        * move virtual); the constructor builds the unit action at +auth_off with a one-province path */
       CTOR_UNIT_PROVINCE = 24,
       /* engineer 118 (28 Nov): volunteers. The action's own constructor has the SIMPLE signature (action, const tag *sender,
        * const tag *receiver, const CDate *) and also builds a division handle list (CPdxArray with an inline buffer:
        * {data -> inline, capacity, count} and its allocator) that the diplomacy view's dialog fills with the chosen
        * divisions: send (CSendVolunteerAction) and recall (CRecallVolunteerAction) */
       CTOR_VOLUNTEER = 25 };
/* engineer 74: BUILD_FIELDS also fills CPdxArray fields (a division handle list, a province list): the fresh object's
 * array is empty ({data 0, capacity 0, size 0} and its allocator set by the factory); the module allocates the
 * elements through THAT allocator's own allocate slot (read from the game's array growth code), so the command's
 * destructor frees them the game's way. Only the module's fresh command object is written, never game state. */
/* how the command's authorisation slot names the objects: through a typed resolver of a handle (the default), or as
 * "the sender controls the country tag at +auth_off" (the objects are then checked by the cast the "can do" makes) */
enum { AUTH_HANDLE = 0, AUTH_TAG = 1,
       /* engineer 75: the sender must control the tag (as AUTH_TAG); the object is a raw pointer of exactly the class
        * obj_cast names (its RTTI vtable), e.g. a national focus the command carries by pointer and writes by name */
       AUTH_TAG_PTR = 2,
       /* engineer 76: an event answer (CSelectEventOptionCommand): the sender must control the country at +auth_off, the
        * event handle resolves to a CEvent with options, and the game state's pending-event record of the command's id
        * holds that event. The object the request names is the open event WINDOW (exact vtable of obj_cast); the
        * module takes every field from it, as the window's own option click does */
       AUTH_EVENT = 3,
       /* engineer 78: a diplomatic action (CDiplomaticActionCommand): the command holds the action by pointer at
        * +auth_off and the sender must control the action's sender tag. obj_cast = the ACTION class (exact vtable); the
        * game's own "can do" is the command's check, which asks the action's own legality virtual (the one that greys the
        * diplomacy view's button) */
       AUTH_DIPLO = 4,
       /* engineer 105: the command holds a unit action at +auth_off (CMoveCommand). The auth slot has the generic shape
        * (the action's resolver, the owner getter, "sender controls"); the resolver reads the action's unit handle through
        * the generic lookup and steps back over the reference base (target_hint): the object is a CUnit, checked as the
        * division commands check one (the lookup gives it back, the C++ cast to CUnit accepts it) */
       AUTH_ACTION = 5 };
/* the second object a type takes, checked before posting */
enum { OBJ2_NONE = 0, OBJ2_AIR_BASE, OBJ2_REGION, OBJ2_ORDERS_GROUP,
       OBJ2_REGION_ID, /* a strategic region given by id in vals (0 = none: hold), looked up as the game's check does */
       OBJ2_FRONT,       /* engineer 74: a CFront of the group's own theatre (exact vtable, its handle resolves to it) */
       OBJ2_CAST,
       OBJ2_SAME_PTR,
       OBJ2_PTR_EXACT }; /* engineer 85: a raw pointer of exactly the class obj2_cast names (its RTTI vtable), written by FK_PTR2 */  /* engineer 79: a raw pointer of exactly the class of the first object (the idea a law replaces) */      /* engineer 75: an object the type's own "can do" casts to spec->obj2_cast (its handle resolves to it,
                          * the C++ cast with the check's own typeinfo accepts it); optional where the spec says so */

/* def: the factory's default for an FK_I32 / FK_U8 / FK_I64 field (checked before the module writes the field; 0 unless
 * given). vi (engineer 74): 1-based index of the request's vals that fills this scalar; 0 = the older rule (the value of
 * the object's own row). keep: 1 = the field keeps the factory's default (checked, not written); -1 (engineer 85) = the
 * factory leaves the field unwritten (the UI's constructor fills it): not checked, written by the module. */
typedef struct { const char *name; uint16_t off; uint8_t kind; uint16_t count_off; uint32_t field_id; int32_t def; int8_t vi; int8_t keep; } cmd_field;

typedef struct {
    const char *family;      /* allow-list unit: "production", "air", "airwing" (a family = one live pass) */
    const char *op;          /* the request name: "production.add_factories" */
    const char *cls;         /* RTTI class name without the length prefix */
    uint32_t size;           /* object size the factory allocates */
    int build, ctor;         /* BUILD_*, CTOR_* */
    int auth_array;          /* 0: one handle at auth_off; 1: handle array(s) starting at auth_off */
    uint16_t auth_off;
    const char *target;      /* Itanium name of the class the typed resolver casts to ("8CAirWing") */
    int nf; cmd_field f[8];  /* BUILD_CTOR: f[0] is the container the fresh object must hold empty */
    int obj2;                /* OBJ2_* */
    int one_cmd;             /* 1: all objects go in ONE command (CTOR_ARRAY); 0: one command per object */
    int auth_kind;           /* AUTH_* */
    const char *obj_cast;    /* AUTH_TAG: the class the "can do" check casts the objects to ("17CEquipmentVariant") */
    const char *obj2_cast;   /* OBJ2_CAST: the class the "can do" check casts the second object to ("11CArmyLeader") */
    int obj2_optional;       /* OBJ2_CAST: 1 = no second object is legal (a general unassigned) */
} cmd_spec;

enum { /* confidence bits: a type is on only when all are set */
    C_FACTORY = 1, C_SIZE = 2, C_TYPEID = 4, C_SLOTS = 8, C_AUTH = 16, C_RESOLVER = 32, C_SERIAL = 64, C_CTOR = 128,
    C_ALL_FIELDS = C_FACTORY | C_SIZE | C_TYPEID | C_SLOTS | C_AUTH | C_RESOLVER | C_SERIAL
};

typedef struct {
    int ok;
    const cmd_spec *spec;
    uint64_t factory, vtable, valid_fn, auth_fn, write_fn, ctor;   /* unslid */
    uint64_t resolve_fn, owner_fn, controls_fn, lookup_fn;        /* from the auth slot / the typed resolver */
    uint32_t type_id, size, slots, target_hint;                   /* hint: offset of the reference base in the object */
    uint64_t ti_src, ti_dst;                                      /* AUTH_TAG: CReferenceObject -> obj_cast typeinfos */
    uint64_t vt_obj;                                              /* AUTH_TAG_PTR: the object's exact vtable */
    uint64_t ti2; uint32_t hint2;                                 /* OBJ2_CAST: the check's typeinfo and hint */
    uint64_t vt_obj2;                                             /* OBJ2_PTR_EXACT: the second object's exact vtable */
    /* engineer 78, AUTH_DIPLO: the action class's constructor, object size (from its own Clone), type id (its SetType),
     * the base constructor it calls, the sender field (from the command's auth slot); WAR: the war goal handle field and
     * the flag byte the constructor writes */
    uint64_t act_ctor, act_base; uint32_t act_size, act_type, act_sender_off, act_goal_off, act_flag_off;
    /* engineer 105, AUTH_ACTION: inside the unit action (at +auth_off): the unit handle the resolver reads, the path
     * (data, count) the province constructor fills, the second path array (data) that must be empty too, and the first
     * of its flag bytes (act_flag_off: +0 normal, +1, +2, +3, +4, +5, +8 kind (i32), +0xc), each read from the action's
     * own constructors; act_ctor = that province constructor */
    uint32_t act_handle_off, act_path_off, act_path_cnt_off, act_path2_off;
    /* engineer 118, CTOR_VOLUNTEER: the action's division handle list: data pointer offset, the inline buffer it points to
     * after construction, and the inline capacity (the list's count is at +0xc of the list), each from the constructor */
    uint32_t act_units_off, act_units_inline, act_units_cap;
    unsigned conf, need;
    char evidence[300];
} cmd_type;

/* What every stage-2 post needs besides the per-type facts, each read from the game's code. */
typedef struct {
    int ok;
    uint32_t valid_slot;      /* vtable byte offset of the "can do" check, from the AI post helper (0x50) */
    uint32_t delete_slot;     /* deleting destructor slot used on a refused command (0x8) */
    uint32_t exec_flag_off;   /* session byte set while it executes commands (0x624), from Post */
    uint32_t allow_flag_off;  /* session byte that lets nested posts through (0x66e), from Post */
    uint32_t player_tag_off;  /* game-state field the per-frame gate compares (> 0: a country is played) (0x4d8) */
    uint32_t gate_flag_off;   /* the session gate byte it sets (0x66d; equals post_layout.gate_flag_off) */
    uint64_t null_ref_global; /* the empty handle the factories copy (unslid) */
    uint32_t cref_handle_off; /* handle offset inside a CReferenceObject (8) */
    /* family facts */
    uint32_t line_factories_off;  /* CEquipmentProductionLine: assigned factories (read by the add-factories check) */
    uint32_t region_id_off;       /* CStrategicRegion: the id the air commands carry (transfer ctor) */
    uint64_t vt_region, vt_base;  /* RTTI primary vtables of CStrategicRegion and CAirBase (unslid) */
    /* engineer 71: a strategic region by id, the way the game's own region check reads it (EnableMission's "can
     * do": gamestate[+regions_arr][id] for 0 < id < gamestate[+regions_cnt]): a mission over a region with no own
     * base, and hold (region 0) */
    uint32_t regions_arr_off, regions_cnt_off;
    /* typeinfo objects for a checked cast of a second object (attach: the army is a COrdersGroup) */
    uint64_t ti_reference, ti_orders_group;
    /* engineer 74: the CPdxArray allocator's allocate slot (alloc(allocator, bytes, align)), by consensus of the game's
     * array growth sites; a division (CUnit) as Assign's own check reads it: the cast's typeinfo and hint, its group */
    uint32_t alloc_slot;
    uint64_t ti_unit; uint32_t unit_hint, unit_group_off;
    /* engineer 75: army structure. A division's theatre as the UI reads it before it builds a new army (the getter the
     * new-army button calls on the first selected division); an orders group's parent group, as COrderAssignCommand's
     * execution and the UI's transfer rule walk it */
    uint32_t unit_theatre_off, group_parent_ptr_off;
    char evidence[300];
} stage2_common;


/* engineer 76: the event window and the pending-event records, each read from the code that uses it:
 *   - the option click of the event window (the only caller of the answer's constructor thunk): the window's pending id,
 *     event, country, scope, the "accepts a click" byte, and the option's own index field;
 *   - the window's per-frame update (a virtual of the window's class): the event's options array and count, the
 *     window's flag byte and the game's own option test it calls for every option (the same function decides what the
 *     window shows);
 *   - the answer's authorisation slot: the event's typeinfo, the pending-event lookup over the game state (array, count,
 *     record size, the record's event pointer) */
typedef struct {
    int ok;
    uint64_t vt_window;                 /* CEventWindow primary vtable (unslid) */
    uint64_t thunk, avail_fn;           /* the constructor's thunk; option test (option, scope *, flag) (unslid) */
    uint64_t ti_reference, ti_event;    /* the auth slot's cast CReferenceObject -> CEvent */
    uint32_t win_id_off, win_event_off, win_country_off, win_scope_off, win_open_off, win_flag_off;
    uint32_t ev_handle_off, ev_opts_off, ev_nopts_off, opt_index_off;
    uint32_t pend_arr_off, pend_cnt_off, pend_size, pend_event_off;
    uint32_t cmd_id_off, cmd_country_off, cmd_option_off;
    char evidence[300];
} events_facts;

/* engineer 78: what every diplomatic action needs, each read from the code that uses it:
 *   - the command maker the diplomacy UI calls with the new action (it writes the game date into the action and moves the
 *     action into a fresh CDiplomaticActionCommand: its make-unique helper holds the command's vtable): make_cmd, the game
 *     state's date field and the action's date field;
 *   - the CDate object layout the action constructors build (two vtables: the primary +0x10 and the secondary +0x60 of
 *     the class's vtable group), read from an action constructor;
 *   - the player's war goals as the declare-war dialog lists them (its selection handler): the country of a tag, the
 *     country's war-goal holder, its list (data, count), a war goal's target tag; the handler writes the chosen goal's
 *     handle into the action at the war constructor's handle field */
typedef struct {
    int ok, war_ok;
    uint64_t make_cmd, make_unique, date_vt;
    uint32_t date_off, act_date_off;
    uint64_t country_of;
    uint32_t wg_holder_off, wg_list_off, wg_count_off, wg_target_off;
    char evidence[300];
} diplo_facts;

/* engineer 79: an area-defence order (COrderInstance of the area kind), each fact from the code that uses it: the
 * state-click command's execution creates the order with "create order of kind K" (mov esi,K), walks the order's state
 * list (+area_off data, +area_cnt count: toggles each named state) and stores the settings through a one-store setter
 * (+setting_off) */
typedef struct {
    int ok;
    uint32_t kind_area, area_off, area_cnt, setting_off;
    char evidence[200];
} holds_facts;

/* engineer 82: a building reference as the game's own reference constructors build it from (template, state) and
 * (template, province), read from both (identical on 1.19.4.1 and 1.20.0.0): the template must be a real one (its byte
 * +tmpl_real_off set), its province-level count +tmpl_prov_off decides the location kind (> 0: a province id, else a
 * state id), its id +tmpl_id_off goes into the reference. The reference's own class vtable (18CBuildingReference) is the
 * one the command's constructor stores */
/* engineer 82: training: the "can train" check the deployment view makes before its Train click (the add-line command's
 * own "can do" calls it with the line count): can_train(country deployment, template body, count, 0); the country from
 * the tag (country_of), its deployment status at +deploy_off, the template's body at +tmpl_body_off */
typedef struct {
    int ok;
    uint64_t can_train, country_of, template_of;
    uint32_t deploy_off, tmpl_body_off;
    char evidence[200];
} training_facts;

typedef struct {
    int ok;
    uint32_t tmpl_real_off, tmpl_prov_off, tmpl_id_off, state_id_off, prov_id_off;
    uint64_t ref_vt;            /* CBuildingReference primary vtable (unslid; the address the ctor stores) */
    char evidence[200];
} construction_facts;

/* engineer 84: military-industrial organisations, each fact read from the code that uses it (identical on 1.20.0.0 and
 * 1.19.4.1):
 *   - the country's organisation list as the "any/every organisation" trigger collects it: country_of(tag) -> country
 *     +mgr_off (the organisation manager) -> organisations +orgs_off (COrganisation *), count +orgs_cnt;
 *   - the country's policies as the attach command's execution finds one: manager +pol_off (CPolicy *), count +pol_cnt,
 *     a policy's token +8;
 *   - COrganisation (exact class): owner tag +tag_off and attached policy token +policy_off (the attach execution), the
 *     key token +key_off (the unlock check names it), funds +funds_off (int64, the add-funds routine), size +size_off and
 *     unspent trait points +points_off (the size-up and unlock routines), the unlocked-trait set +unlocked_off (a libc++
 *     std::set of STraitId: begin, end node, size), the unlock queue +queue_off / +queue_cnt (STraitId 0x10: token +8),
 *     the trait tree +tree_off (array +tree_arr / +tree_cnt of trait templates of stride trait_stride, token +8);
 *   - the game's own functions: tok2str(token) -> const std::string * (the unlock check's name), can_unlock(org, trait,
 *     &unlocked, 1, NULL) (the unlock command's "can do"), the policy list item's click gate (the UI's legality of attaching
 *     or detaching: item +gate_policy_off policy token, +gate_org_off organisation handle);
 *   - a production line's organisation handle +line_org_off (the line's organisation setter) */
typedef struct {
    int ok;
    uint64_t country_of, tok2str, can_unlock, policy_gate, vt_org;
    uint32_t mgr_off, orgs_off, orgs_cnt, pol_off, pol_cnt;
    uint32_t tag_off, policy_off, key_off, funds_off, size_off, points_off, unlocked_off, queue_off, queue_cnt;
    uint32_t tree_off, tree_arr, tree_cnt, trait_stride, gate_policy_off, gate_org_off, line_org_off;
    int32_t empty_tok; /* the factory's "no trait / no policy" token (the command fields' default; detach sends it) */
    char evidence[400];
} mio_facts;

#define CMD2_MAX 160
/* engineer 122: every diplomatic action class the game has (the RTTI subclasses of CDiplomaticAction, two levels), with
 * the shape its player constructor has. A buildable shape becomes one row of the table, family `diplomacy2`, op
 * "diplomacy2.<key>" (key = the class name in snake case without C and Action), built through the same spec machinery
 * as the hand-written diplomacy rows; a class a hand-written row already covers is listed with that row's op; any other
 * class is listed "unsupported:<shape>" with its evidence, never hidden. A new game version's new action class appears
 * without a code change when its constructor has a known shape. */
#define DIPLO_CAT_MAX 64
typedef struct {
    char cls[72];        /* the Itanium name ("16CGuaranteeAction") */
    char key[64];        /* "guarantee" */
    char op[80];         /* the op that posts it ("diplomacy.guarantee" or "diplomacy2.embargo"); empty = unsupported */
    char shape[48];      /* simple | divisions | war_goal | unsupported:<why> */
    uint32_t act_type;   /* the type id its constructor stamps (0 when not read) */
    int on;              /* the op resolved on this build */
} diplo_class;

typedef struct {
    stage2_common common;
    orders_facts orders;      /* engineer 73: the battle-plan order tree (core/orders.c) */
    fronts_facts fronts;      /* engineer 74: the theatre fronts a plan root binds to (core/orders.c) */
    events_facts events;      /* engineer 76: the event window and the pending-event records */
    diplo_facts diplo;        /* engineer 78: the command maker, the date, the war goals */
    holds_facts holds;        /* engineer 79: the area-defence order */
    construction_facts build; /* engineer 82: the building reference */
    training_facts train;     /* engineer 82: the Train click's "can train" */
    mio_facts mio;            /* engineer 84: the organisations */
    int n;
    cmd_type t[CMD2_MAX];
    /* engineer 122: the diplomatic action catalog and the specs of its rows (the rows are t[n - ndyn .. n - 1]) */
    diplo_class dcat[DIPLO_CAT_MAX];
    cmd_spec dspec[DIPLO_CAT_MAX];
    int ndcat, ndyn;
} stage2_table;

extern const cmd_spec CMD2_SPECS[];
extern const int CMD2_NSPECS;

int scan_stage2_common(const scan_image *im, const post_layout *pl, uint64_t gs_global, stage2_common *out);
int scan_cmd_type(const scan_image *im, const post_layout *pl, const stage2_common *c, const cmd_spec *s, cmd_type *out);
/* Resolve every spec; returns the number of types on. */
int scan_stage2(const scan_image *im, const post_layout *pl, uint64_t gs_global, stage2_table *t);
const cmd_type *stage2_find(const stage2_table *t, const char *op);
/* engineer 122: the catalog as JSON members ("actions":[...]) for the module's diplomacy_actions read */
const char *stage2_diplo_json(const stage2_table *t, char *buf, size_t n);
/* 1 if every type of the family is on */
int stage2_family_on(const stage2_table *t, const char *family);
const char *stage2_json(const stage2_table *t, char *buf, size_t n);
#endif
