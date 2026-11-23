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

enum { FK_I32 = 1, FK_U8, FK_REF, FK_REF_ARRAY };
/* How the module builds the command:
 *   BUILD_FIELDS: the game's factory makes the default object; the module writes the spec's scalar and handle fields
 *                 into that fresh object (each field's default and serializer entry checked first).
 *   BUILD_CTOR:   payloads with containers: the module calls the UI's own constructor of the type (found by scan, the
 *                 one whose argument stores hit the serializer's offsets) on the factory's fresh object, so the game's
 *                 code allocates and fills its own containers. */
enum { BUILD_FIELDS = 1, BUILD_CTOR };
enum { CTOR_NONE = 0, CTOR_WING_I32_U8 = 1, CTOR_WING_I32_NULL = 2 };

typedef struct { const char *name; uint16_t off; uint8_t kind; uint16_t count_off; uint32_t field_id; } cmd_field;

typedef struct {
    const char *family;      /* allow-list unit: "production", "air" */
    const char *op;          /* the request name: "production.add_factories" */
    const char *cls;         /* RTTI class name without the length prefix */
    uint32_t size;           /* object size the factory allocates */
    int build, ctor;         /* BUILD_*, CTOR_* */
    int auth_array;          /* 0: one handle at auth_off; 1: handle array(s) starting at auth_off */
    uint16_t auth_off;
    const char *target;      /* Itanium name of the class the typed resolver casts to ("8CAirWing") */
    int nf; cmd_field f[5];
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
    char evidence[300];
} stage2_common;

#define CMD2_MAX 8
typedef struct {
    stage2_common common;
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
