/* HOI4 Live Tools in-process module: the capability table (stage 0).
 * One entry per capability, resolved at load by scanning (or, for object field offsets that no scan derives yet,
 * from a per-build table keyed by the image UUID). A capability that does not resolve is off; nothing that depends
 * on it runs, and the external runtime keeps serving that need. */
#ifndef HOI4_INPROC_CAPS_H
#define HOI4_INPROC_CAPS_H
#include "scan.h"

enum {
    CAP_POST,            /* CCommandQueue::Post (not called in stage 0) */
    CAP_QUEUE_ROOT,      /* the idler global the UI posts through, and the queue getter's vtable slot */
    CAP_RESEARCH_FACTORY,/* CCommandHelper<CSetResearchCommand>::CommandRegistrator::Create (not called in stage 0) */
    CAP_RESEARCH_VTABLE,
    CAP_RESEARCH_TYPE_ID,
    CAP_GAMESTATE_VTABLE,/* RTTI 17CCurrentGameState: finds the game-state root by dynamic type */
    CAP_IDLER_VTABLE,    /* RTTI 12CInGameIdler */
    CAP_FIELDS,          /* per-build field offsets for the three stage-0 reads */
    CAP_COUNT
};

typedef struct {
    const char *uuid, *version;
    uint64_t date, speed, player_tag;   /* CCurrentGameState fields */
    uint64_t paused;                    /* CInGameIdler field */
    uint64_t tag_db_global;             /* unslid image address of the tag name array root */
    uint64_t tag_stride;
} caps_fields;

typedef struct {
    scan_cap cap[CAP_COUNT];
    const caps_fields *fields;          /* NULL: unknown build, reads off */
} caps_table;

/* Resolve every capability on an image view. Returns the number resolved. */
int caps_resolve(const scan_image *im, caps_table *t);
/* The whole table as one JSON object (buf >= 8 KiB). */
const char *caps_json(const scan_image *im, const caps_table *t, char *buf, size_t n);
#endif
