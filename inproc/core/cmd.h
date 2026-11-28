/* HOI4 Live Tools in-process module: the command layer's scans (stage 1).
 *
 * Pure computation over an image view, like scan.c. Each scan proves, from the game's own code, what the module needs
 * before it may post ONE command type the way the UI does:
 *   - post_layout: what CSession::Post (the function the UI posts through) checks and stamps, read from its code:
 *     the drop gate's command slot, the sender field it copies into the command, the server it hands the command to,
 *     the authorisation slot it asks. Slot numbers move between builds (1.19.4.1 auth 0x98, 1.20 0xa0), so they are
 *     read here, never assumed.
 *   - speed_cmd: CSetGameSpeedCommand's factory, RTTI-verified vtable, protocol type id, object size, its ONE payload
 *     field (offset from the factory's default store, the validity check and the serializer must all agree), the
 *     valid range, that Post's drop gate passes it, the sender its authorisation requires, and the game-state field
 *     its Execute writes (the read-back).
 *   - forbid_tlv: the accessor of the game's thread-local _ThreadForbidCount (the game's own rule for which thread may
 *     touch game state), by voting over every call site of its assertion.
 * A scan that does not match its exact pattern leaves the capability off; nothing is guessed. */
#ifndef HOI4_INPROC_CMD_H
#define HOI4_INPROC_CMD_H
#include "scan.h"

typedef struct {
    int ok;
    uint64_t post;             /* unslid */
    uint32_t gate_flag_off;    /* session byte that lets any command through (0x66d) */
    uint32_t gate_slot;        /* command vtable byte offset: "may be posted anywhere" (0x88) */
    uint32_t sender_src;       /* session int copied into command+0xc (0x84) */
    uint32_t server_off;       /* session pointer to the server the command is handed to (0x40) */
    uint32_t send_slot;        /* server vtable byte offset of the hand-over (0x30) */
    uint32_t auth_slot;        /* command vtable byte offset of the authorisation check (0x98 / 0xa0) */
    char evidence[200];
} post_layout;

typedef struct {
    int ok;
    uint64_t factory, vtable;  /* unslid */
    uint32_t type_id, size, slots;
    uint32_t payload_off;      /* the one int32 field the UI fills (speed index) */
    uint32_t range_max;        /* exclusive: valid values 0 .. range_max-1 */
    uint32_t field_id;         /* serializer id of the field (protocol) */
    uint32_t required_sender;  /* authorisation: command+0xc must equal this */
    uint32_t speed_field;      /* game-state offset Execute writes (read-back) */
    uint64_t gs_global;        /* unslid global Execute reads the game state from */
    char evidence[240];
} speed_cmd;

typedef struct {
    int ok;
    uint64_t accessor;         /* unslid: push rbp; lea rdi,[desc]; call [rdi]; pop rbp; ret */
    uint64_t desc;             /* unslid TLV descriptor */
    unsigned votes, refs;
    char evidence[160];
} forbid_tlv;

int scan_post_layout(const scan_image *im, uint64_t post, post_layout *out);
int scan_speed_command(const scan_image *im, const post_layout *pl, speed_cmd *out);
int scan_forbid_tlv(const scan_image *im, forbid_tlv *out);

/* Masked byte pattern: space-separated tokens; "xx" a hex byte, "??" any byte, "B" capture one byte, "D" capture a
 * little-endian 32-bit value. Returns 1 on a match at b (n bytes available) and fills caps in order. */
int cmd_pmatch(const uint8_t *b, size_t n, const char *pat, uint32_t *caps, int maxcaps);
#endif
