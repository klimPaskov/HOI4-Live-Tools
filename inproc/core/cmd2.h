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
       FK_TAG, FK_PTR };
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
       CTOR_GROUPS = 15 };     /* (cmd, const CPdxArray<COrdersGroup *> *): armies out of their army group */
/* engineer 74: BUILD_FIELDS also fills CPdxArray fields (a division handle list, a province list): the fresh object's
 * array is empty ({data 0, capacity 0, size 0} and its allocator set by the factory); the module allocates the
 * elements through THAT allocator's own allocate slot (read from the game's array growth code), so the command's
 * destructor frees them the game's way. Only the module's fresh command object is written, never game state. */
/* how the command's authorisation slot names the objects: through a typed resolver of a handle (the default), or as
 * "the sender controls the country tag at +auth_off" (the objects are then checked by the cast the "can do" makes) */
enum { AUTH_HANDLE = 0, AUTH_TAG = 1,
       /* engineer 75: the sender must control the tag (as AUTH_TAG); the object is a raw pointer of exactly the class
        * obj_cast names (its RTTI vtable), e.g. a national focus the command carries by pointer and writes by name */
       AUTH_TAG_PTR = 2 };
/* the second object a type takes, checked before posting */
enum { OBJ2_NONE = 0, OBJ2_AIR_BASE, OBJ2_REGION, OBJ2_ORDERS_GROUP,
       OBJ2_REGION_ID, /* a strategic region given by id in vals (0 = none: hold), looked up as the game's check does */
       OBJ2_FRONT,       /* engineer 74: a CFront of the group's own theatre (exact vtable, its handle resolves to it) */
       OBJ2_CAST };      /* engineer 75: an object the type's own "can do" casts to spec->obj2_cast (its handle resolves to it,
                          * the C++ cast with the check's own typeinfo accepts it); optional where the spec says so */

/* def: the factory's default for an FK_I32 / FK_U8 / FK_I64 field (checked before the module writes the field; 0 unless
 * given). vi (engineer 74): 1-based index of the request's vals that fills this scalar; 0 = the older rule (the value of
 * the object's own row). keep: 1 = the field keeps the factory's default (checked, not written). */
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


#define CMD2_MAX 48
typedef struct {
    stage2_common common;
    orders_facts orders;      /* engineer 73: the battle-plan order tree (core/orders.c) */
    fronts_facts fronts;      /* engineer 74: the theatre fronts a plan root binds to (core/orders.c) */
    int n;
    cmd_type t[CMD2_MAX];
} stage2_table;

extern const cmd_spec CMD2_SPECS[];
extern const int CMD2_NSPECS;

int scan_stage2_common(const scan_image *im, const post_layout *pl, uint64_t gs_global, stage2_common *out);
int scan_cmd_type(const scan_image *im, const post_layout *pl, const stage2_common *c, const cmd_spec *s, cmd_type *out);
/* Resolve every spec; returns the number of types on. */
int scan_stage2(const scan_image *im, const post_layout *pl, uint64_t gs_global, stage2_table *t);
const cmd_type *stage2_find(const stage2_table *t, const char *op);
/* 1 if every type of the family is on */
int stage2_family_on(const stage2_table *t, const char *family);
const char *stage2_json(const stage2_table *t, char *buf, size_t n);
#endif
