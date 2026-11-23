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
     {{"line", 0x24, FK_REF, 0, 0x2fb3}, {"delta", 0x2c, FK_I32, 0, 0x2f76}}},
    /* production: a line's close button */
    {"production", "production.remove_line", "CRemoveProductionLineCommand", 0x30, BUILD_FIELDS, CTOR_NONE, 0, 0x24,
     "24CEquipmentProductionLine", 1, {{"line", 0x24, FK_REF, 0, 0x2fb3}}},
    /* air: rebase. The UI's rebase helper builds, per selected wing not already at the base, Transfer(wing, base, 0, NULL, 0) */
    {"air", "air.transfer", "CStratAirTransferCommand", 0x40, BUILD_FIELDS, CTOR_NONE, 0, 0x24, "8CAirWing", 2,
     {{"wing", 0x24, FK_REF, 0, 0x2fb5}, {"base", 0x2c, FK_REF, 0, 0x2fb6}}},
    /* air: a mission bit on a wing (the mission toolbar; the UI posts one per selected wing: (wing, 1<<mission, enable)) */
    {"air", "air.set_mission", "CStratAirSetMissionCommand", 0x48, BUILD_CTOR, CTOR_WING_I32_U8, 1, 0x28, "8CAirWing", 3,
     {{"wings", 0x28, FK_REF_ARRAY, 0x34, 0x3ba6}, {"mission_mask", 0x40, FK_I32, 0, 0xe1}, {"enable", 0x44, FK_U8, 0, 0x2888}}},
    /* air: a wing to a strategic region (the region right click; ctor (wing, region id, NULL)) */
    {"air", "air.enable_mission", "CStratAirEnableMissionCommand", 0x60, BUILD_CTOR, CTOR_WING_I32_NULL, 1, 0x28,
     "8CAirWing", 2, {{"wings", 0x28, FK_REF_ARRAY, 0x34, 0x2fb5}, {"region_id", 0x58, FK_I32, 0, 0x2eee}}},
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

/* the serializer writes field id `id` (mov esi, id) with the field's offset as a displacement close before it */
static int serial_has(const scan_image *im, uint64_t fn, uint32_t id, uint16_t off) {
    const uint8_t *b = scan_ptr(im, fn, 0x200);
    if (!b) return 0;
    for (size_t i = 0; i + 5 <= 0x200; i++) {
        if (b[i] != 0xbe || rd32(b + i + 1) != id) continue;
        for (size_t j = i > 24 ? i - 24 : 0; j < i + 24 && j + 1 < 0x200; j++) /* ModRM with an 8-bit displacement = the field offset */
            if (j != i && off < 0x80 && b[j + 1] == off && (b[j] & 0xc0) == 0x40) return 1;
        return 0;
    }
    return 0;
}

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
    if ((o->conf & C_SLOTS) && auth_shape(im, o->auth_fn, s, o)) o->conf |= C_AUTH;
    if ((o->conf & C_AUTH) && resolver_shape(im, s, o)) o->conf |= C_RESOLVER;
    if (o->conf & C_SLOTS) {
        int all = 1;
        for (int i = 0; i < s->nf; i++) {
            uint16_t off = s->f[i].kind == FK_REF_ARRAY ? s->f[i].count_off : s->f[i].off;
            if (!serial_has(im, o->write_fn, s->f[i].field_id, off)) all = 0;
        }
        if (all) o->conf |= C_SERIAL;
    }
    if (s->build == BUILD_CTOR && (o->conf & C_FACTORY) && find_ctor(im, s, o)) o->conf |= C_CTOR;
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

int scan_stage2(const scan_image *im, const post_layout *pl, uint64_t gs_global, stage2_table *t) {
    memset(t, 0, sizeof *t);
    scan_stage2_common(im, pl, gs_global, &t->common);
    int on = 0;
    for (int i = 0; i < CMD2_NSPECS && i < CMD2_MAX; i++) {
        on += scan_cmd_type(im, pl, &t->common, &CMD2_SPECS[i], &t->t[i]);
        t->n++;
    }
    /* every type must agree on the "sender controls" check and the generic lookup (one game, one rule) */
    for (int i = 1; i < t->n; i++)
        if (t->t[i].ok && t->t[0].ok &&
            (t->t[i].controls_fn != t->t[0].controls_fn || t->t[i].lookup_fn != t->t[0].lookup_fn)) {
            t->t[i].ok = 0; on--;
            snprintf(t->t[i].evidence + strlen(t->t[i].evidence), sizeof t->t[i].evidence - strlen(t->t[i].evidence),
                     "; disagrees with the other types' authorisation helpers");
        }
    /* family facts the per-family checks and read-backs need */
    for (int i = 0; i < t->n; i++) {
        const char *fam = t->t[i].spec->family;
        if (!strcmp(fam, "production") && !t->common.line_factories_off && t->t[i].ok) { t->t[i].ok = 0; on--; }
        if (!strcmp(t->t[i].spec->op, "air.enable_mission") && !t->common.region_id_off && t->t[i].ok) { t->t[i].ok = 0; on--; }
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
                                 "\"handle_off\":%u,\"line_factories_off\":%u,\"region_id_off\":%u},\"types\":[",
                                 c->ok ? "true" : "false", c->valid_slot, c->delete_slot, c->exec_flag_off, c->allow_flag_off,
                                 c->player_tag_off, c->gate_flag_off, (unsigned long long)c->null_ref_global,
                                 c->cref_handle_off, c->line_factories_off, c->region_id_off);
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
    if (at < n) snprintf(buf + at, n - at, "]}");
    return buf;
}
