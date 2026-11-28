/* HOI4 Live Tools in-process module: the command layer's scans. See cmd.h. Pure computation over an image view. */
#include "cmd.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static uint32_t rd32(const uint8_t *p) { uint32_t v; memcpy(&v, p, 4); return v; }
static int32_t rds32(const uint8_t *p) { int32_t v; memcpy(&v, p, 4); return v; }

static int hexv(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    return -1;
}

int cmd_pmatch(const uint8_t *b, size_t n, const char *pat, uint32_t *caps, int maxcaps) {
    size_t at = 0; int nc = 0;
    for (const char *p = pat; *p;) {
        while (*p == ' ') p++;
        if (!*p) break;
        if (p[0] == '?' && p[1] == '?') { if (at + 1 > n) return 0; at++; p += 2; continue; }
        if (p[0] == 'B') { if (at + 1 > n || nc >= maxcaps) return 0; caps[nc++] = b[at]; at++; p++; continue; }
        if (p[0] == 'D') { if (at + 4 > n || nc >= maxcaps) return 0; caps[nc++] = rd32(b + at); at += 4; p++; continue; }
        int h = hexv(p[0]), l = hexv(p[1]);
        if (h < 0 || l < 0) return 0; /* malformed pattern: never a match */
        if (at + 1 > n || b[at] != (uint8_t)(h * 16 + l)) return 0;
        at++; p += 2;
    }
    return 1;
}

/* first offset in b[0..n) where pat matches; counts all matches in *count */
static long pfind(const uint8_t *b, size_t n, const char *pat, uint32_t *caps, int maxcaps, unsigned *count) {
    long first = -1; unsigned c = 0; uint32_t tmp[8];
    for (size_t i = 0; i < n; i++)
        if (cmd_pmatch(b + i, n - i, pat, first < 0 ? caps : tmp, maxcaps)) { if (first < 0) first = (long)i; c++; }
    if (count) *count = c;
    return first;
}

static uint64_t find_cstring(const scan_image *im, const char *s) {
    const uint8_t *b = scan_ptr(im, im->cstr_addr, im->cstr_size);
    size_t n = strlen(s) + 1;
    if (!b || im->cstr_size < n) return 0;
    for (uint64_t i = 0; i + n <= im->cstr_size; i++)
        if (b[i] == (uint8_t)s[0] && !memcmp(b + i, s, n) && (i == 0 || b[i - 1] == 0)) return im->cstr_addr + i;
    return 0;
}

/* does code in [a, a+len) load the address of target with lea r64,[rip+d32]? */
static int lea_to(const scan_image *im, uint64_t a, size_t len, uint64_t target) {
    const uint8_t *t = scan_ptr(im, a, len);
    if (!t) return 0;
    for (size_t i = 0; i + 7 <= len; i++)
        if ((t[i] == 0x48 || t[i] == 0x4c) && t[i + 1] == 0x8d && (t[i + 2] & 0xc7) == 0x05 &&
            a + i + 7 + (int64_t)rds32(t + i + 3) == target) return 1;
    return 0;
}

static int in_text(const scan_image *im, uint64_t a) { return a >= im->text_addr && a < im->text_addr + im->text_size; }

int scan_post_layout(const scan_image *im, uint64_t post, post_layout *o) {
    memset(o, 0, sizeof *o);
    o->post = post;
    const uint8_t *b = post ? scan_ptr(im, post, 0x300) : NULL;
    if (!b) { snprintf(o->evidence, sizeof o->evidence, "post function unresolved"); return 0; }
    uint32_t c[8];
    /* prologue: test edx (force); session gate byte; command gate slot; second slot; sender stamp into cmd+0xc */
    const char *P = "55 48 89 e5 41 57 41 56 41 54 53 48 81 ec ?? ?? ?? ?? 48 89 f3 49 89 fe 85 d2 75 ?? "
                    "41 80 be D 00 75 ?? 48 8b 03 48 89 df ff 90 D 84 c0 75 ?? 48 8b 03 48 89 df ff 50 ?? 84 c0 "
                    "0f 84 ?? ?? ?? ?? 41 8b 86 D 89 43 0c";
    if (!cmd_pmatch(b, 0x300, P, c, 8)) { snprintf(o->evidence, sizeof o->evidence, "post prologue differs from the known shape"); return 0; }
    o->gate_flag_off = c[0]; o->gate_slot = c[1]; o->sender_src = c[2];
    unsigned n1 = 0, n2 = 0;
    long s = pfind(b, 0x300, "49 8b 7e B 48 8b 07 41 b6 01 48 89 de ff 50 B", c, 8, &n1);
    if (s < 0) { snprintf(o->evidence, sizeof o->evidence, "no server hand-over in post"); return 0; }
    o->server_off = c[0]; o->send_slot = c[1];
    long a = pfind(b, 0x300, "48 8b 03 48 8d 75 ?? 48 89 df ff 90 D", c, 8, &n2);
    if (a < 0 || n2 != 1) { snprintf(o->evidence, sizeof o->evidence, "authorisation call not unique in post (%u)", n2); return 0; }
    o->auth_slot = c[0];
    uint64_t sna = find_cstring(im, "Dropped (sender not authorized): ");
    int named = sna && lea_to(im, post, 0x800, sna);
    o->ok = named && o->gate_slot % 8 == 0 && o->auth_slot % 8 == 0 && o->send_slot % 8 == 0 && o->gate_slot < 0x200 &&
            o->auth_slot < 0x200 && o->sender_src < 0x4000 && o->server_off < 0x4000 && o->gate_flag_off < 0x4000;
    snprintf(o->evidence, sizeof o->evidence,
             "gate [s+0x%x] slot 0x%x; sender [s+0x%x]->cmd+0xc; server [s+0x%x] slot 0x%x; auth slot 0x%x; %s",
             o->gate_flag_off, o->gate_slot, o->sender_src, o->server_off, o->send_slot, o->auth_slot,
             named ? "drop string referenced" : "drop string NOT referenced");
    return o->ok;
}

#define SPEED_CLASS "CSetGameSpeedCommand"

int scan_speed_command(const scan_image *im, const post_layout *pl, speed_cmd *o) {
    memset(o, 0, sizeof *o);
    scan_cap f, vt, tid;
    if (!pl || !pl->ok) { snprintf(o->evidence, sizeof o->evidence, "post layout unresolved"); return 0; }
    if (!scan_command_factory(im, SPEED_CLASS, &f, &vt, &tid)) {
        snprintf(o->evidence, sizeof o->evidence, "factory/vtable/type id: %s | %s | %s", f.evidence, vt.evidence, tid.evidence);
        return 0;
    }
    o->factory = f.value; o->vtable = vt.value; o->type_id = (uint32_t)tid.value; o->size = (uint32_t)f.extra;
    o->slots = (uint32_t)vt.extra;
    /* factory: operator new(size), the common 0x24-byte header, the vptr, then exactly ONE int32 default store */
    const uint8_t *fb = scan_ptr(im, o->factory, 0x60);
    uint32_t c[8];
    if (!fb || !cmd_pmatch(fb, 0x60, "55 48 89 e5 bf D e8 ?? ?? ?? ?? c6 40 08 00 c7 40 0c ff ff ff ff 48 c7 40 10 00 00 00 00 "
                                       "66 c7 40 18 ff ff 48 c7 40 1a 00 00 00 00 66 c7 40 22 00 00 48 8d 0d D 48 89 08 "
                                       "c7 40 B 00 00 00 00 5d c3", c, 8)) {
        snprintf(o->evidence, sizeof o->evidence, "factory body is not header + one int field");
        return 0;
    }
    uint64_t vstore = o->factory + 0x3c + (int64_t)(int32_t)c[1]; /* the lea rcx,[rip+d] ends at +0x3c */
    o->payload_off = c[2];
    if (c[0] != o->size || vstore != o->vtable || o->size != o->payload_off + 4) {
        snprintf(o->evidence, sizeof o->evidence, "factory size 0x%x / vptr 0x%llx / payload +0x%x disagree", c[0],
                 (unsigned long long)vstore, o->payload_off);
        return 0;
    }
    unsigned nvalid = 0, nser = 0, nexec = 0;
    uint32_t setter_field = 0, clamp = 0; uint64_t gs = 0, setter = 0;
    for (uint32_t k = 0; k < o->slots; k++) {
        uint64_t fn;
        if (!scan_q(im, o->vtable + 8 * k, &fn) || !in_text(im, fn)) continue;
        const uint8_t *x = scan_ptr(im, fn, 0x100);
        if (!x) continue;
        if (cmd_pmatch(x, 16, "55 48 89 e5 83 7f B B 0f 92 c0 5d c3", c, 8) && c[0] == o->payload_off) {
            nvalid++; o->range_max = c[1];
        }
        if (cmd_pmatch(x, 20, "55 48 89 e5 8b 57 B 48 89 f7 be D 5d e9", c, 8) && c[0] == o->payload_off) {
            nser++; o->field_id = c[1];
        }
        for (size_t i = 0; i + 18 <= 0x100; i++) /* Execute: lea rax,[rip+G]; mov rdi,[rax]; mov esi,[rbx+off]; call S */
            if (cmd_pmatch(x + i, 18, "48 8d 05 D 48 8b 38 8b 73 B e8 D", c, 8) && c[1] == o->payload_off) {
                gs = fn + i + 7 + (int64_t)(int32_t)c[0];       /* lea ends at +7 */
                setter = fn + i + 18 + (int64_t)(int32_t)c[2];  /* 7 + 3 + 3 + call(5) = 18 */
                nexec++;
                break;
            }
    }
    if (nexec == 1) {
        const uint8_t *s = scan_ptr(im, setter, 0x30);
        if (s && cmd_pmatch(s, 0x30, "55 48 89 e5 41 56 53 48 83 ec 10 83 fe B b8 D 0f 42 c6 31 c9 85 f6 0f 49 c8 89 8f D", c, 8)) {
            clamp = c[0]; setter_field = c[2];
            if (c[1] != clamp) clamp = 0xffffffff;
        }
    }
    o->speed_field = setter_field; o->gs_global = gs;
    /* what Post will ask of this command */
    uint64_t gate_fn = 0, auth_fn = 0, base = 0;
    int gate_true = 0, auth_ok = 0;
    if (scan_q(im, o->vtable + pl->gate_slot, &gate_fn) && in_text(im, gate_fn)) {
        const uint8_t *g = scan_ptr(im, gate_fn, 8);
        gate_true = g && cmd_pmatch(g, 8, "55 48 89 e5 b0 01 5d c3", c, 8);
    }
    if (scan_q(im, o->vtable + pl->auth_slot, &auth_fn) && in_text(im, auth_fn)) {
        const uint8_t *a = scan_ptr(im, auth_fn, 16);
        if (a && cmd_pmatch(a, 16, "55 48 89 e5 41 56 53 48 89 f3 e8 D", c, 8)) {
            base = auth_fn + 15 + (int64_t)(int32_t)c[0];
            const uint8_t *bb = scan_ptr(im, base, 16);
            if (bb && cmd_pmatch(bb, 16, "55 48 89 e5 83 7f 0c B 0f 94 c0 5d c3", c, 8)) { auth_ok = 1; o->required_sender = c[0]; }
        }
    }
    o->ok = nvalid == 1 && nser == 1 && nexec == 1 && setter_field && setter_field < 0x4000 && o->range_max >= 2 &&
            o->range_max <= 16 && clamp == o->range_max - 1 && gate_true && auth_ok && gs >= im->data_lo && gs < im->data_hi;
    snprintf(o->evidence, sizeof o->evidence,
             "size 0x%x, payload int32 +0x%x (factory/valid/serializer agree: %u/%u), valid 0..%u, field id 0x%x, "
             "Execute writes gs+0x%x (clamp %u), gate %s, auth sender==%u%s",
             o->size, o->payload_off, nvalid, nser, o->range_max ? o->range_max - 1 : 0, o->field_id, setter_field,
             clamp, gate_true ? "true" : "NOT constant true", o->required_sender, auth_ok ? "" : " (auth NOT matched)");
    return o->ok;
}

int scan_forbid_tlv(const scan_image *im, forbid_tlv *o) {
    memset(o, 0, sizeof *o);
    uint64_t s = find_cstring(im, "_ThreadForbidCount == 0 && \"Current thread is forbidden to access gamestate\"");
    if (!s) { snprintf(o->evidence, sizeof o->evidence, "assertion string absent"); return 0; }
    const uint8_t *t = scan_ptr(im, im->text_addr, im->text_size);
    if (!t) return 0;
    uint64_t n = im->text_size, va = im->text_addr;
    uint64_t cand[4] = {0}, desc[4] = {0}; unsigned votes[4] = {0}, nc = 0;
    for (uint64_t i = 0x30; i + 7 <= n; i++) {
        if ((t[i] != 0x48 && t[i] != 0x4c) || t[i + 1] != 0x8d || (t[i + 2] & 0xc7) != 0x05) continue;
        if (va + i + 7 + (int64_t)rds32(t + i + 3) != s) continue;
        o->refs++;
        for (uint64_t j = i - 0x30; j < i; j++) {
            if (t[j] != 0xe8 || t[j + 5] != 0x83 || t[j + 6] != 0x38 || t[j + 7] != 0x00) continue; /* call F; cmp dword [rax],0 */
            uint64_t f = va + j + 5 + (int64_t)rds32(t + j + 1);
            const uint8_t *fb = scan_ptr(im, f, 16);
            uint32_t c[2];
            if (!fb || !cmd_pmatch(fb, 16, "55 48 89 e5 48 8d 3d D ff 17 5d c3", c, 2)) continue;
            uint64_t d = f + 11 + (int64_t)(int32_t)c[0];
            unsigned k;
            for (k = 0; k < nc && cand[k] != f; k++) {}
            if (k == nc) { if (nc == 4) continue; cand[nc] = f; desc[nc] = d; nc++; }
            votes[k]++;
            break;
        }
    }
    unsigned best = 0, other = 0;
    for (unsigned k = 1; k < nc; k++) if (votes[k] > votes[best]) best = k;
    for (unsigned k = 0; k < nc; k++) if (k != best) other += votes[k];
    if (nc) { o->accessor = cand[best]; o->desc = desc[best]; o->votes = votes[best]; }
    o->ok = nc && o->votes >= 100 && other == 0 && o->desc >= im->data_lo && o->desc < im->data_hi;
    snprintf(o->evidence, sizeof o->evidence, "%u of %u assertion sites call accessor 0x%llx (descriptor 0x%llx); %u disagree",
             o->votes, o->refs, (unsigned long long)o->accessor, (unsigned long long)o->desc, other);
    return o->ok;
}
