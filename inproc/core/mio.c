/* engineer 84: the organisations read (see mio.h) */
#include "mio.h"
#include <stdio.h>
#include <string.h>

#define MIO_MAX_ORGS 64
#define MIO_MAX_TRAITS 256
#define MIO_MAX_POLICIES 64
#define MIO_MAX_SET 512

static int q64(mio_rd rd, uint64_t a, uint64_t *v) { return a && rd(a, v, 8); }
static int r32(mio_rd rd, uint64_t a, int32_t *v) { return a && rd(a, v, 4); }

/* a libc++ std::string (long form: byte 0 bit 0 set, size +8, data +0x10; short form: size = byte 0 >> 1, data +1),
 * copied with JSON-unsafe bytes dropped */
int mio_str(mio_rd rd, uint64_t s, char *buf, size_t n) {
    uint8_t raw[24];
    if (!n) return 0;
    buf[0] = 0;
    if (!s || !rd(s, raw, sizeof raw)) return 0;
    uint64_t len, data;
    char tmp[128];
    if (raw[0] & 1) {
        memcpy(&len, raw + 8, 8); memcpy(&data, raw + 16, 8);
        if (len >= sizeof tmp || !data || !rd(data, tmp, len)) return 0;
    } else {
        len = raw[0] >> 1;
        if (len > 22) return 0;
        memcpy(tmp, raw + 1, len);
    }
    size_t o = 0;
    for (uint64_t i = 0; i < len && o + 1 < n; i++) {
        unsigned char ch = (unsigned char)tmp[i];
        if (ch < 0x20 || ch == '"' || ch == '\\' || ch > 0x7e) continue;
        buf[o++] = (char)ch;
    }
    buf[o] = 0;
    return 1;
}

int mio_in_list(const mio_facts *f, mio_rd rd, uint64_t mgr, uint64_t org) {
    uint64_t arr = 0, x = 0; int32_t cnt = 0;
    if (!q64(rd, mgr + f->orgs_off, &arr) || !r32(rd, mgr + f->orgs_cnt, &cnt) || cnt < 0 || cnt > MIO_MAX_ORGS) return 0;
    for (int32_t i = 0; i < cnt; i++) if (q64(rd, arr + 8 * (uint64_t)i, &x) && x == org) return 1;
    return 0;
}

uint64_t mio_trait(const mio_facts *f, mio_rd rd, uint64_t org, int32_t token) {
    uint64_t tree = 0, arr = 0; int32_t cnt = 0, t = 0;
    if (!q64(rd, org + f->tree_off, &tree) || !q64(rd, tree + f->tree_arr, &arr) || !r32(rd, tree + f->tree_cnt, &cnt) ||
        cnt < 0 || cnt > MIO_MAX_TRAITS)
        return 0;
    for (int32_t i = 0; i < cnt; i++) {
        uint64_t tr = arr + (uint64_t)i * f->trait_stride;
        if (r32(rd, tr + 8, &t) && t == token) return tr;
    }
    return 0;
}

uint64_t mio_policy(const mio_facts *f, mio_rd rd, uint64_t mgr, int32_t token) {
    uint64_t arr = 0, p = 0; int32_t cnt = 0, t = 0;
    if (!q64(rd, mgr + f->pol_off, &arr) || !r32(rd, mgr + f->pol_cnt, &cnt) || cnt < 0 || cnt > MIO_MAX_POLICIES) return 0;
    for (int32_t i = 0; i < cnt; i++) if (q64(rd, arr + 8 * (uint64_t)i, &p) && p && r32(rd, p + 8, &t) && t == token) return p;
    return 0;
}

int mio_queued(const mio_facts *f, mio_rd rd, uint64_t org, int32_t token) {
    uint64_t arr = 0; int32_t cnt = 0, t = 0;
    if (!q64(rd, org + f->queue_off, &arr) || !r32(rd, org + f->queue_cnt, &cnt) || cnt < 0 || cnt > MIO_MAX_TRAITS) return -1;
    for (int32_t i = 0; i < cnt; i++) {
        if (!r32(rd, arr + 0x10 * (uint64_t)i + 8, &t)) return -1;
        if (t == token) return 1;
    }
    return 0;
}

/* the unlocked set: a libc++ std::set<STraitId> {begin node, end node (its left child is the root), size}; node: left,
 * right, parent, colour, then the value (STraitId: vptr, token +8) at +0x20. Every node visited once, bounded by size. */
static int set_tokens(const mio_facts *f, mio_rd rd, uint64_t org, int32_t *out, int max) {
    uint64_t base = org + f->unlocked_off, root = 0, stack[64]; uint64_t size = 0;
    if (!q64(rd, base + 0x10, &size) || size > MIO_MAX_SET || !q64(rd, base + 8, &root)) return -1;
    int sp = 0, n = 0;
    if (root) stack[sp++] = root;
    while (sp) {
        uint64_t nd = stack[--sp], l = 0, r = 0; int32_t t = 0;
        if (!rd(nd, &l, 8) || !rd(nd + 8, &r, 8) || !r32(rd, nd + 0x28, &t)) return -1;
        if (n >= max || (uint64_t)n >= size) return -1;
        out[n++] = t;
        if (l) { if (sp >= 64) return -1; stack[sp++] = l; }
        if (r) { if (sp >= 64) return -1; stack[sp++] = r; }
    }
    return (uint64_t)n == size ? n : -1;
}

int mio_unlocked(const mio_facts *f, mio_rd rd, uint64_t org, int32_t token) {
    int32_t toks[MIO_MAX_SET];
    int n = set_tokens(f, rd, org, toks, MIO_MAX_SET);
    if (n < 0) return -1;
    for (int i = 0; i < n; i++) if (toks[i] == token) return 1;
    return 0;
}

#define PUT(...) do { if (at < cap) at += (size_t)snprintf(out + at, cap - at, __VA_ARGS__); } while (0)

int mio_json(const mio_facts *f, const mio_env *e, uint64_t mgr, int32_t player_tag, char *out, size_t cap) {
    size_t at = 0;
    uint64_t arr = 0, parr = 0; int32_t cnt = 0, pcnt = 0;
    mio_rd rd = e->rd;
    if (!f->ok || !q64(rd, mgr + f->orgs_off, &arr) || !r32(rd, mgr + f->orgs_cnt, &cnt) || cnt < 0 || cnt > MIO_MAX_ORGS ||
        !q64(rd, mgr + f->pol_off, &parr) || !r32(rd, mgr + f->pol_cnt, &pcnt) || pcnt < 0 || pcnt > MIO_MAX_POLICIES)
        return -1;
    char nm[128];
    /* the country's policies: token, key, base cost (+0x40 as the cost routine reads it) */
    uint64_t pols[MIO_MAX_POLICIES]; int32_t ptok[MIO_MAX_POLICIES];
    PUT("\"policies\":[");
    for (int32_t i = 0; i < pcnt; i++) {
        int32_t cost = 0;
        if (!q64(rd, parr + 8 * (uint64_t)i, &pols[i]) || !pols[i] || !r32(rd, pols[i] + 8, &ptok[i])) return -1;
        r32(rd, pols[i] + 0x40, &cost);
        mio_str(rd, e->name(e->ctx, ptok[i]), nm, sizeof nm);
        PUT("%s{\"token\":%d,\"key\":\"%s\",\"cost\":%d}", i ? "," : "", ptok[i], nm, cost);
    }
    PUT("],\"empty_token\":%d,\"organisations\":[", f->empty_tok);
    int n = 0;
    for (int32_t i = 0; i < cnt; i++) {
        uint64_t org = 0, vt = 0, h = 0, tree = 0, tarr = 0, qarr = 0; int32_t tag = 0, key = 0, size = 0, pts = 0, pol = 0,
                 tcnt = 0, qcnt = 0;
        int64_t funds = 0;
        if (!q64(rd, arr + 8 * (uint64_t)i, &org) || !q64(rd, org, &vt) || vt != e->vt_org) return -1;
        if (!r32(rd, org + f->tag_off, &tag) || tag != player_tag) continue; /* the player's own only */
        if (!q64(rd, org + 8, &h) || !r32(rd, org + f->key_off, &key) || !rd(org + f->funds_off, &funds, 8) ||
            !r32(rd, org + f->size_off, &size) || !r32(rd, org + f->points_off, &pts) || !r32(rd, org + f->policy_off, &pol) ||
            !q64(rd, org + f->tree_off, &tree) || !q64(rd, tree + f->tree_arr, &tarr) || !r32(rd, tree + f->tree_cnt, &tcnt) ||
            tcnt < 0 || tcnt > MIO_MAX_TRAITS || !q64(rd, org + f->queue_off, &qarr) || !r32(rd, org + f->queue_cnt, &qcnt) ||
            qcnt < 0 || qcnt > MIO_MAX_TRAITS)
            return -1;
        int32_t un[MIO_MAX_SET];
        int nun = set_tokens(f, rd, org, un, MIO_MAX_SET);
        if (nun < 0) return -1;
        mio_str(rd, e->name(e->ctx, key), nm, sizeof nm);
        PUT("%s{\"org\":\"0x%llx\",\"handle\":\"0x%llx\",\"key\":\"%s\",\"funds_raw\":%lld,\"size\":%d,\"trait_points\":%d,"
            "\"policy\":%d,", n ? "," : "", (unsigned long long)org, (unsigned long long)h, nm, (long long)funds, size, pts, pol);
        if (pol != f->empty_tok) { mio_str(rd, e->name(e->ctx, pol), nm, sizeof nm); PUT("\"policy_key\":\"%s\",", nm); }
        PUT("\"policies_allowed\":[");
        int np = 0;
        for (int32_t p = 0; p < pcnt; p++)
            if (e->policy_ok(e->ctx, org, h, ptok[p])) PUT("%s%d", np++ ? "," : "", ptok[p]);
        PUT("],\"queue\":[");
        int32_t qt[MIO_MAX_TRAITS];
        for (int32_t q = 0; q < qcnt; q++) {
            if (!r32(rd, qarr + 0x10 * (uint64_t)q + 8, &qt[q])) return -1;
            PUT("%s%d", q ? "," : "", qt[q]);
        }
        /* the tree in the game's order: [token, key, state] (u unlocked, q queued, a can be unlocked now, l locked) */
        PUT("],\"unlocked\":%d,\"traits\":[", nun);
        for (int32_t t = 0; t < tcnt; t++) {
            uint64_t tr = tarr + (uint64_t)t * f->trait_stride; int32_t tok = 0;
            if (!r32(rd, tr + 8, &tok)) return -1;
            char st = 'l';
            for (int u = 0; u < nun && st == 'l'; u++) if (un[u] == tok) st = 'u';
            for (int32_t q = 0; q < qcnt && st == 'l'; q++) if (qt[q] == tok) st = 'q';
            if (st == 'l' && e->can_unlock(e->ctx, org, tr)) st = 'a';
            mio_str(rd, e->name(e->ctx, tok), nm, sizeof nm);
            PUT("%s[%d,\"%s\",\"%c\"]", t ? "," : "", tok, nm, st);
        }
        PUT("]}");
        n++;
        if (at + 64 >= cap) break;
    }
    PUT("],\"organisations_read\":%d,\"truncated\":%s", n, at + 64 >= cap ? "true" : "false");
    if (at >= cap) return -1;
    return n;
}
