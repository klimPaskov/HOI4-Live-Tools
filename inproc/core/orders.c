/* HOI4 Live Tools in-process module: the battle-plan order tree (scan + pure reader). See orders.h. */
#include "orders.h"
#include "cmd.h"
#include <stdio.h>
#include <string.h>

static int32_t rds32(const uint8_t *p) { int32_t v; memcpy(&v, p, 4); return v; }
static int in_text(const scan_image *im, uint64_t a) { return a >= im->text_addr && a < im->text_addr + im->text_size; }

static long find_in(const scan_image *im, uint64_t a, size_t len, const char *pat, uint32_t *caps, int maxcaps, unsigned *count) {
    const uint8_t *b = scan_ptr(im, a, len);
    long first = -1; unsigned c = 0; uint32_t tmp[8];
    if (count) *count = 0;
    if (!b) return -1;
    for (size_t i = 0; i < len; i++)
        if (cmd_pmatch(b + i, len - i, pat, first < 0 ? caps : tmp, maxcaps)) { if (first < 0) first = (long)i; c++; }
    if (count) *count = c;
    return first;
}

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

static uint64_t fn_start(const scan_image *im, uint64_t a) {
    if (!im->fstarts || !im->nfstarts) return 0;
    size_t lo = 0, hi = im->nfstarts;
    while (hi - lo > 1) { size_t mid = (lo + hi) / 2; if (im->fstarts[mid] <= a) lo = mid; else hi = mid; }
    return im->fstarts[lo] <= a ? im->fstarts[lo] : 0;
}

/* the target of a rel32 call/jmp whose 4-byte displacement starts at `at` */
static uint64_t rel_target(uint64_t at, uint32_t disp) { return at + 4 + (int64_t)(int32_t)disp; }

int scan_orders_facts(const scan_image *im, orders_facts *f) {
    memset(f, 0, sizeof *f);
    char *ev = f->evidence; size_t evn = sizeof f->evidence;
    uint32_t c[8]; unsigned n;
    scan_cap vt, fac, cvt, tid;
    /* 1. COrderInstance: vtable by RTTI, then its constructor by the vtable store + the kind/owner stores and the
     *    id store followed by clearing the executing byte (the order starts not executing) */
    if (!scan_class_vtable(im, "14COrderInstance", &vt)) { snprintf(ev, evn, "COrderInstance vtable not found"); return 0; }
    f->vt_order = vt.value;
    uint64_t st[16]; unsigned ns = lea_sites(im, vt.value, st, 16);
    for (unsigned i = 0; i < ns && i < 16 && !f->id_off; i++) {
        uint64_t fs = fn_start(im, st[i]);
        if (!fs || st[i] - fs > 0x80) continue;
        if (find_in(im, st[i], 0x60, "89 77 B 48 89 57 B", c, 8, &n) < 0 || n != 1) continue;
        uint32_t kind = c[0], owner = c[1];
        if (find_in(im, fs, 0x400, "45 89 be D 41 c6 86 D 00", c, 8, &n) < 0 || n != 1) continue;
        f->kind_off = kind; f->owner_off = owner; f->id_off = c[0]; f->exec_off = c[1];
    }
    if (!f->id_off) { snprintf(ev, evn, "COrderInstance constructor (kind/owner/id stores) not found"); return 0; }
    /* 2. the order lookup the commands' own checks call: COrderSetPathCommand's "can do" (lookup(group, [cmd+0x2c]));
     *    the same check reads the lock handle and compares it with the empty handle */
    if (!scan_command_factory(im, "COrderSetPathCommand", &fac, &cvt, &tid)) { snprintf(ev, evn, "COrderSetPathCommand unresolved"); return 0; }
    uint64_t valid = 0, exec = 0;
    if (!scan_q(im, cvt.value + 0x50, &valid) || !in_text(im, valid) || !scan_q(im, cvt.value + 0x58, &exec) || !in_text(im, exec)) {
        snprintf(ev, evn, "SetPath slots unreadable"); return 0;
    }
    long p = find_in(im, valid, 0x100, "8b 73 2c 48 89 c7 e8 D 48 85 c0", c, 8, &n);
    if (p < 0 || n != 1) { snprintf(ev, evn, "order lookup call not found in SetPath's check"); return 0; }
    f->lookup_fn = rel_target(valid + (uint64_t)p + 7, c[0]);
    if (find_in(im, valid, 0x100, "8b 90 D 48 8d 0d D 3b 51 04 75 ?? 8b 80 D 3b 01", c, 8, &n) >= 0 && n == 1 && c[0] == c[2] + 4)
        f->lock_off = c[2];
    /* 3. the lookup: three lists, each "cmp [r14+CNT],0 ; jle ; xor r15d,r15d" then "mov rax,[r14+DATA]; mov rdi,
     *    [rax+8*r15]; mov esi,ebx; call subtree" */
    const uint8_t *lb = scan_ptr(im, f->lookup_fn, 0x100);
    int nl = 0;
    for (size_t k = 0; lb && k + 9 < 0x100 && nl < 3; k++) {
        if (!cmd_pmatch(lb + k, 0x100 - k, "41 83 be D 00 7e ?? 45 31 ff", c, 8)) continue;
        uint32_t cnt = c[0];
        long q = find_in(im, f->lookup_fn + k, 0x30, "49 8b 86 D 4a 8b 3c f8 89 de e8 D", c, 8, &n);
        if (q < 0) break;
        uint64_t sub = rel_target(f->lookup_fn + k + (uint64_t)q + 14, c[1]);
        if (f->subtree_fn && sub != f->subtree_fn) break;
        f->subtree_fn = sub; f->list_cnt[nl] = cnt; f->list_off[nl] = c[0]; nl++;
    }
    if (nl != 3) { snprintf(ev, evn, "the lookup's three order lists not found (%d)", nl); return 0; }
    /* 4. the subtree search: id compare, then the two child lists searched recursively */
    if (find_in(im, f->subtree_fn, 0x30, "39 b7 D 0f 84 D", c, 8, &n) < 0 || c[0] != f->id_off) {
        snprintf(ev, evn, "the subtree search does not compare the constructor's id field"); return 0;
    }
    int nc = 0;
    const uint8_t *sb = scan_ptr(im, f->subtree_fn, 0xa0);
    for (size_t k = 0; sb && k + 16 < 0xa0 && nc < 2; k++)
        if (cmd_pmatch(sb + k, 0xa0 - k, "44 8b bb D 45 85 ff 7e ?? 4c 8b a3 D", c, 8)) { f->child_cnt[nc] = c[0]; f->child_off[nc] = c[1]; nc++; }
    if (nc != 2) { snprintf(ev, evn, "the subtree search's child lists not found (%d)", nc); return 0; }
    /* 5. the path setter (SetPath's execution tail-calls it): the order's province list and its divisions */
    p = find_in(im, exec, 0x60, "41 b8 01 00 00 00 5b 41 5e 5d e9 D", c, 8, &n);
    if (p < 0) { snprintf(ev, evn, "SetPath's execution does not jump to the path setter"); return 0; }
    f->path_fn = rel_target(exec + (uint64_t)p + 11, c[0]);
    if (find_in(im, f->path_fn, 0x600, "83 7a B 00 7e ?? 48 ff c1 48 8b 72 B", c, 8, &n) >= 0) { f->provs_cnt = c[0]; f->provs_off = c[1]; }
    if (find_in(im, f->path_fn, 0x300, "49 63 85 D 48 39 c3 7d ?? 49 8b 85 D", c, 8, &n) >= 0) { f->divs_cnt = c[0]; f->divs_off = c[1]; }
    /* 6. COrderNewFrontCommand's execution: the flags and the drawing group it writes into each new order */
    if (scan_command_factory(im, "COrderNewFrontCommand", &fac, &cvt, &tid) && scan_q(im, cvt.value + 0x58, &exec) && in_text(im, exec)) {
        if (find_in(im, exec, 0x400, "41 0f b6 46 6d 88 81 D 41 0f b6 46 6e 88 81 D", c, 8, &n) >= 0) { f->blitz_off = c[0]; f->withdraw_off = c[1]; }
        if (find_in(im, exec, 0x400, "48 8b 45 ?? 48 89 81 D", c, 8, &n) >= 0) f->drawer_off = c[0];
    }
    f->ok = f->vt_order && f->lookup_fn && in_text(im, f->lookup_fn) && in_text(im, f->subtree_fn) && f->kind_off && f->owner_off &&
            f->id_off && f->exec_off == f->id_off + 4 && f->provs_off && f->provs_cnt == f->provs_off + 0xc && f->divs_off &&
            f->divs_cnt == f->divs_off + 0xc && f->child_cnt[0] == f->child_off[0] + 0xc && f->child_cnt[1] == f->child_off[1] + 0xc &&
            f->list_cnt[0] == f->list_off[0] + 0xc && f->list_cnt[1] == f->list_off[1] + 0xc && f->list_cnt[2] == f->list_off[2] + 0xc &&
            f->blitz_off && f->withdraw_off == f->blitz_off + 1 && f->drawer_off && f->lock_off;
    snprintf(ev, evn,
             "vt 0x%llx lookup 0x%llx subtree 0x%llx path 0x%llx; kind +0x%x owner +0x%x id +0x%x exec +0x%x; lists +0x%x/+0x%x/+0x%x; "
             "children +0x%x/+0x%x; provinces +0x%x; divisions +0x%x; blitz +0x%x withdraw +0x%x drawer +0x%x lock +0x%x",
             (unsigned long long)f->vt_order, (unsigned long long)f->lookup_fn, (unsigned long long)f->subtree_fn,
             (unsigned long long)f->path_fn, f->kind_off, f->owner_off, f->id_off, f->exec_off, f->list_off[0], f->list_off[1],
             f->list_off[2], f->child_off[0], f->child_off[1], f->provs_off, f->divs_off, f->blitz_off, f->withdraw_off,
             f->drawer_off, f->lock_off);
    return f->ok;
}

const char *orders_facts_json(const orders_facts *f, char *buf, size_t n) {
    snprintf(buf, n,
             "{\"ok\":%s,\"vt_order\":\"0x%llx\",\"lookup\":\"0x%llx\",\"kind\":%u,\"owner\":%u,\"id\":%u,\"exec\":%u,"
             "\"lists\":[%u,%u,%u],\"children\":[%u,%u],\"provinces\":%u,\"divisions\":%u,\"blitz\":%u,\"withdraw\":%u,"
             "\"drawer\":%u,\"lock\":%u}",
             f->ok ? "true" : "false", (unsigned long long)f->vt_order, (unsigned long long)f->lookup_fn, f->kind_off,
             f->owner_off, f->id_off, f->exec_off, f->list_off[0], f->list_off[1], f->list_off[2], f->child_off[0],
             f->child_off[1], f->provs_off, f->divs_off, f->blitz_off, f->withdraw_off, f->drawer_off, f->lock_off);
    return buf;
}

/* ---- the pure reader ---- */
typedef struct { char *out; size_t n, at; int trunc; } wbuf;
static void w(wbuf *b, const char *fmt, ...) __attribute__((format(printf, 2, 3)));
#include <stdarg.h>
static void w(wbuf *b, const char *fmt, ...) {
    if (b->trunc || b->at >= b->n) { b->trunc = 1; return; }
    va_list ap; va_start(ap, fmt);
    int k = vsnprintf(b->out + b->at, b->n - b->at, fmt, ap);
    va_end(ap);
    if (k < 0 || (size_t)k >= b->n - b->at) { b->trunc = 1; b->out[b->at] = 0; return; }
    b->at += (size_t)k;
}

/* a CPdxArray header {data, capacity, size}: data at off, size at cnt */
static int arr(orders_rd rd, uint64_t obj, uint32_t off, uint32_t cnt, uint64_t *data, int32_t *size) {
    return rd(obj + off, data, 8) && rd(obj + cnt, size, 4) && *size >= 0 && *size < 100000;
}

static int node_json(const orders_facts *f, orders_rd rd, uint64_t vt, uint64_t o, int depth, wbuf *b) {
    uint64_t v = 0, data = 0, drawer = 0, lock = 0; int32_t kind = 0, id = 0, size = 0; uint8_t ex = 0, bl = 0, wd = 0;
    if (!rd(o, &v, 8) || v != vt) return -1;
    if (!rd(o + f->kind_off, &kind, 4) || !rd(o + f->id_off, &id, 4) || !rd(o + f->exec_off, &ex, 1) || !rd(o + f->blitz_off, &bl, 1) ||
        !rd(o + f->withdraw_off, &wd, 1) || !rd(o + f->drawer_off, &drawer, 8) || !rd(o + f->lock_off, &lock, 8))
        return -1;
    int count = 1;
    w(b, "{\"order\":\"0x%llx\",\"kind\":%d,\"id\":%d,\"executing\":%s,\"blitz\":%s,\"withdraw\":%s,\"locked\":%s,\"drawer\":\"0x%llx\",\"provinces\":[",
      (unsigned long long)o, kind, id, ex ? "true" : "false", bl ? "true" : "false", wd ? "true" : "false", lock ? "true" : "false",
      (unsigned long long)drawer);
    if (!arr(rd, o, f->provs_off, f->provs_cnt, &data, &size)) return -1;
    for (int32_t i = 0; i < size && i < 4096; i++) { int32_t pv = 0; if (!rd(data + 4 * (uint64_t)i, &pv, 4)) return -1; w(b, "%s%d", i ? "," : "", pv); }
    if (!arr(rd, o, f->divs_off, f->divs_cnt, &data, &size)) return -1;
    w(b, "],\"divisions\":%d,\"division_ptrs\":[", size);
    for (int32_t i = 0; i < size && i < 256; i++) { uint64_t d = 0; if (!rd(data + 8 * (uint64_t)i, &d, 8)) return -1; w(b, "%s\"0x%llx\"", i ? "," : "", (unsigned long long)d); }
    for (int l = 0; l < 2; l++) {
        w(b, "],\"%s\":[", l ? "target_copies" : "children");
        if (!arr(rd, o, f->child_off[l], f->child_cnt[l], &data, &size)) return -1;
        for (int32_t i = 0; depth < 4 && i < size && i < 32; i++) {
            uint64_t ch = 0;
            if (!rd(data + 8 * (uint64_t)i, &ch, 8)) return -1;
            if (i) w(b, ",");
            int k = node_json(f, rd, vt, ch, depth + 1, b);
            if (k < 0) return -1;
            count += k;
        }
    }
    w(b, "]}");
    return count;
}

int orders_tree_json(const orders_facts *f, orders_rd rd, uint64_t vt, uint64_t group, char *out, size_t n) {
    static const char *const names[3] = {"top", "fallback", "area"};
    wbuf b = {out, n, 0, 0};
    int total = 0;
    if (!f->ok || n < 64) return -1;
    out[0] = 0;
    w(&b, "\"group\":\"0x%llx\",\"lists\":[", (unsigned long long)group);
    for (int l = 0; l < 3; l++) {
        uint64_t data = 0; int32_t size = 0;
        if (!arr(rd, group, f->list_off[l], f->list_cnt[l], &data, &size)) return -1;
        w(&b, "%s{\"list\":\"%s\",\"orders\":[", l ? "," : "", names[l]);
        for (int32_t i = 0; i < size && i < 64; i++) {
            uint64_t o = 0;
            if (!rd(data + 8 * (uint64_t)i, &o, 8)) return -1;
            if (i) w(&b, ",");
            int k = node_json(f, rd, vt, o, 0, &b);
            if (k < 0) return -1;
            total += k;
        }
        w(&b, "]}");
    }
    w(&b, "]");
    int trunc = b.trunc;
    if (trunc) { b.trunc = 0; b.at = 0; w(&b, "\"group\":\"0x%llx\",\"lists\":[]", (unsigned long long)group); }
    w(&b, ",\"orders_read\":%d,\"truncated\":%s", total, trunc ? "true" : "false");
    return total;
}

static uint64_t find_node(const orders_facts *f, orders_rd rd, uint64_t vt, uint64_t o, int32_t id, int depth) {
    uint64_t v = 0, data = 0; int32_t oid = 0, size = 0;
    if (!rd(o, &v, 8) || v != vt || !rd(o + f->id_off, &oid, 4)) return 0;
    if (oid == id) return o;
    for (int l = 0; l < 2 && depth < 4; l++) {
        if (!arr(rd, o, f->child_off[l], f->child_cnt[l], &data, &size)) return 0;
        for (int32_t i = 0; i < size && i < 32; i++) {
            uint64_t ch = 0, r;
            if (rd(data + 8 * (uint64_t)i, &ch, 8) && (r = find_node(f, rd, vt, ch, id, depth + 1))) return r;
        }
    }
    return 0;
}

uint64_t orders_find(const orders_facts *f, orders_rd rd, uint64_t vt, uint64_t group, int32_t id) {
    if (!f->ok || id == 0) return 0;
    for (int l = 0; l < 3; l++) {
        uint64_t data = 0; int32_t size = 0;
        if (!arr(rd, group, f->list_off[l], f->list_cnt[l], &data, &size)) return 0;
        for (int32_t i = 0; i < size && i < 64; i++) {
            uint64_t o = 0, r;
            if (rd(data + 8 * (uint64_t)i, &o, 8) && (r = find_node(f, rd, vt, o, id, 0))) return r;
        }
    }
    return 0;
}

int orders_free_divisions(const orders_facts *f, orders_rd rd, uint64_t parent, uint64_t *out, int max) {
    uint64_t pd = 0, cd = 0; int32_t pn = 0, cn = 0;
    int k = 0;
    if (!arr(rd, parent, f->divs_off, f->divs_cnt, &pd, &pn) || !arr(rd, parent, f->child_off[0], f->child_cnt[0], &cd, &cn)) return -1;
    for (int32_t i = 0; i < pn && i < 1024; i++) {
        uint64_t d = 0; int held = 0;
        if (!rd(pd + 8 * (uint64_t)i, &d, 8)) return -1;
        for (int32_t c = 0; c < cn && c < 64 && !held; c++) {
            uint64_t ch = 0, chd = 0; int32_t chn = 0;
            if (!rd(cd + 8 * (uint64_t)c, &ch, 8) || !arr(rd, ch, f->divs_off, f->divs_cnt, &chd, &chn)) return -1;
            for (int32_t e = 0; e < chn && e < 1024 && !held; e++) { uint64_t x = 0; if (!rd(chd + 8 * (uint64_t)e, &x, 8)) return -1; held = x == d; }
        }
        if (!held) { if (k >= max) return -1; out[k++] = d; }
    }
    return k;
}

/* ---- engineer 74 (24 Nov): the fronts a plan root binds to (see orders.h) ---- */
int scan_fronts_facts(const scan_image *im, fronts_facts *f) {
    memset(f, 0, sizeof *f);
    char *ev = f->evidence; size_t evn = sizeof f->evidence;
    uint32_t c[8]; unsigned n;
    scan_cap vt, fac, cvt, tid;
    if (!scan_class_vtable(im, "8CTheatre", &vt)) { snprintf(ev, evn, "CTheatre vtable not found"); return 0; }
    f->vt_theatre = vt.value;
    if (!scan_class_vtable(im, "6CFront", &vt)) { snprintf(ev, evn, "CFront vtable not found"); return 0; }
    f->vt_front = vt.value;
    if (!scan_class_vtable(im, "13CFrontSection", &vt)) { snprintf(ev, evn, "CFrontSection vtable not found"); return 0; }
    f->vt_section = vt.value;
    if (!scan_class_vtable(im, "12COrdersGroup", &vt)) { snprintf(ev, evn, "COrdersGroup vtable not found"); return 0; }
    f->vt_group = vt.value;
    /* 1. the COrdersGroup constructor: vptr, the second vptr (+0x18), a cleared word, the owner tag (*rsi), the parent (rdx) */
    uint64_t st[16]; unsigned ns = lea_sites(im, f->vt_group, st, 16);
    for (unsigned i = 0; i < ns && i < 16 && !f->group_parent_off; i++)
        if (find_in(im, st[i], 0x30, "48 89 07 48 8d 05 D 48 89 47 18 66 c7 47 B 00 00 8b 06 89 47 B 48 89 57 B", c, 8, &n) == 7 && n == 1) {
            f->group_tag_off = c[2]; f->group_parent_off = c[3];
        }
    /* 2. the theatre's fronts list: the loader's "new CFront(theatre)" (vptr, the theatre into +0x18) then the append to
     *    the theatre's array (size at CNT compared with the capacity at CNT-4) */
    ns = lea_sites(im, f->vt_front, st, 16);
    for (unsigned i = 0; i < ns && i < 16 && !f->theatre_fronts_cnt; i++) {
        if (find_in(im, st[i], 0x10, "48 8d 05 D 49 89 07 49 89 5f 18", c, 8, &n) != 0) continue;
        if (find_in(im, st[i], 0x90, "44 8b 63 B 44 3b 63 B", c, 8, &n) >= 0 && c[1] + 4 == c[0]) {
            f->theatre_fronts_cnt = c[0]; f->theatre_fronts_off = c[0] - 0xc;
        }
    }
    /* 3. the root's front link, from COrderNewRootCommand's execution: [cmd+0x48] (the front handle) and [cmd+0x50]
     *    (the section id) into the new root */
    if (scan_command_factory(im, "COrderNewRootCommand", &fac, &cvt, &tid)) {
        uint64_t exec = 0;
        if (scan_q(im, cvt.value + 0x58, &exec) && in_text(im, exec) &&
            find_in(im, exec, 0x120, "41 8b 47 50 49 8b 4f 48 49 89 8e D 41 89 86 D", c, 8, &n) >= 0 && n == 1 && c[1] == c[0] + 8) {
            f->root_front_off = c[0]; f->root_section_off = c[1];
        }
    }
    /* 4. the section lookup every root path update calls with [root + section]: all call sites must agree */
    if (f->root_section_off) {
        char pat[80];
        snprintf(pat, sizeof pat, "8b b3 %02x %02x 00 00 48 89 c7 e8 D", f->root_section_off & 0xff, (f->root_section_off >> 8) & 0xff);
        const uint8_t *t = scan_ptr(im, im->text_addr, im->text_size);
        uint64_t tgt = 0; int agree = 1, sites = 0;
        for (uint64_t i = 0; t && i + 16 <= im->text_size; i++) {
            if (!cmd_pmatch(t + i, 16, pat, c, 8)) continue;
            uint64_t x = rel_target(im->text_addr + i + 10, c[0]);
            if (tgt && x != tgt) agree = 0;
            tgt = x; sites++;
        }
        if (sites && agree && find_in(im, tgt, 0x30, "55 48 89 e5 8b 4f B 85 c9 7e ?? 48 8b 57 B 31 ff", c, 8, &n) == 0 && c[0] == c[1] + 0xc) {
            uint32_t cnt = c[0], off = c[1];
            if (find_in(im, tgt, 0x40, "48 8b 04 fa 39 70 B 74", c, 8, &n) >= 0) {
                f->section_lookup_fn = tgt; f->front_sections_cnt = cnt; f->front_sections_off = off; f->section_id_off = c[0];
            }
        }
    }
    /* 5. a section's provinces: the range-to-index arithmetic reads the count, the copy reads pointer -> id */
    if (find_in(im, im->text_addr, im->text_size, "48 63 47 B 48 ff c8 48 0f af c2 48 69 c0 a0 86 01 00", c, 8, &n) >= 0 && n == 1)
        f->section_provs_cnt = c[0];
    if (find_in(im, im->text_addr, im->text_size, "48 8b 47 B 4a 8b 04 f0 44 8b a0 D", c, 8, &n) >= 0 && n == 1 && c[0] + 0xc == f->section_provs_cnt) {
        f->section_provs_off = c[0]; f->province_id_off = c[1];
    }
    /* 6. the root's range [from, to] (the setter EditRoot's execution calls first, clamping both to 0..100000) */
    if (scan_command_factory(im, "COrderEditRootCommand", &fac, &cvt, &tid)) {
        uint64_t exec = 0;
        long p;
        if (scan_q(im, cvt.value + 0x58, &exec) && in_text(im, exec) &&
            (p = find_in(im, exec, 0x80, "48 8b 73 30 48 8b 53 38 48 89 c7 e8 D", c, 8, &n)) >= 0) {
            uint64_t setter = rel_target(exec + (uint64_t)p + 12, c[0]);
            if (find_in(im, setter, 0x60, "48 89 b7 D 4c 89 87 D 5d c3", c, 8, &n) >= 0 && c[1] == c[0] + 8) f->root_range_off = c[0];
        }
    }
    f->ok = f->group_parent_off && f->group_parent_off < 0x200 && f->group_tag_off && f->theatre_fronts_off && f->front_sections_off &&
            f->section_id_off && f->section_provs_off && f->province_id_off && f->province_id_off < 0x400 && f->root_front_off &&
            f->root_section_off && f->root_range_off;
    snprintf(ev, evn,
             "group parent +0x%x tag +0x%x; theatre fronts +0x%x/+0x%x; front sections +0x%x/+0x%x id +0x%x (lookup 0x%llx); "
             "section provinces +0x%x/+0x%x province id +0x%x; root front +0x%x section +0x%x range +0x%x",
             f->group_parent_off, f->group_tag_off, f->theatre_fronts_off, f->theatre_fronts_cnt, f->front_sections_off,
             f->front_sections_cnt, f->section_id_off, (unsigned long long)f->section_lookup_fn, f->section_provs_off,
             f->section_provs_cnt, f->province_id_off, f->root_front_off, f->root_section_off, f->root_range_off);
    return f->ok;
}

const char *fronts_facts_json(const fronts_facts *f, char *buf, size_t n) {
    snprintf(buf, n,
             "{\"ok\":%s,\"vt_theatre\":\"0x%llx\",\"vt_front\":\"0x%llx\",\"vt_section\":\"0x%llx\",\"group_parent\":%u,"
             "\"group_tag\":%u,\"theatre_fronts\":%u,\"front_sections\":%u,\"section_id\":%u,\"section_provinces\":%u,"
             "\"province_id\":%u,\"root_front\":%u,\"root_section\":%u,\"root_range\":%u}",
             f->ok ? "true" : "false", (unsigned long long)f->vt_theatre, (unsigned long long)f->vt_front,
             (unsigned long long)f->vt_section, f->group_parent_off, f->group_tag_off, f->theatre_fronts_off,
             f->front_sections_off, f->section_id_off, f->section_provs_off, f->province_id_off, f->root_front_off,
             f->root_section_off, f->root_range_off);
    return buf;
}

uint64_t fronts_theatre_of(const fronts_facts *f, orders_rd rd, uint64_t vt_group, uint64_t vt_theatre, uint64_t group) {
    uint64_t g = group, v = 0;
    if (!f->ok) return 0;
    for (int step = 0; step < 4; step++) {
        uint64_t p = 0;
        if (!rd(g + f->group_parent_off, &p, 8) || !p || !rd(p, &v, 8)) return 0;
        if (v == vt_theatre) return p;
        if (v != vt_group) return 0; /* only a plain COrdersGroup parent is followed */
        g = p;
    }
    return 0;
}

static int section_ok(const fronts_facts *f, orders_rd rd, uint64_t vt_section, uint64_t s, int32_t *id, uint64_t *pd, int32_t *pn) {
    uint64_t v = 0;
    return rd(s, &v, 8) && v == vt_section && rd(s + f->section_id_off, id, 4) && arr(rd, s, f->section_provs_off, f->section_provs_cnt, pd, pn);
}

int fronts_json(const fronts_facts *f, orders_rd rd, uint64_t vt_front, uint64_t vt_section, uint64_t theatre, char *out, size_t n) {
    wbuf b = {out, n, 0, 0};
    uint64_t fd = 0; int32_t fn = 0;
    if (!f->ok || n < 64 || !arr(rd, theatre, f->theatre_fronts_off, f->theatre_fronts_cnt, &fd, &fn)) return -1;
    out[0] = 0;
    w(&b, "\"theatre\":\"0x%llx\",\"fronts\":[", (unsigned long long)theatre);
    int nfr = 0;
    for (int32_t i = 0; i < fn && i < 64; i++) {
        uint64_t fr = 0, v = 0, sd = 0; int32_t sn = 0;
        if (!rd(fd + 8 * (uint64_t)i, &fr, 8) || !rd(fr, &v, 8) || v != vt_front ||
            !arr(rd, fr, f->front_sections_off, f->front_sections_cnt, &sd, &sn)) return -1;
        w(&b, "%s{\"front\":\"0x%llx\",\"sections\":[", i ? "," : "", (unsigned long long)fr);
        for (int32_t k = 0; k < sn && k < 64; k++) {
            uint64_t s = 0, pd = 0; int32_t id = 0, pn = 0;
            if (!rd(sd + 8 * (uint64_t)k, &s, 8) || !section_ok(f, rd, vt_section, s, &id, &pd, &pn)) return -1;
            w(&b, "%s{\"id\":%d,\"provinces\":[", k ? "," : "", id);
            for (int32_t e = 0; e < pn && e < 2048; e++) {
                uint64_t pp = 0; int32_t pid = 0;
                if (!rd(pd + 8 * (uint64_t)e, &pp, 8) || !rd(pp + f->province_id_off, &pid, 4)) return -1;
                w(&b, "%s%d", e ? "," : "", pid);
            }
            w(&b, "]}");
        }
        w(&b, "]}");
        nfr++;
    }
    int trunc = b.trunc;
    if (trunc) { b.trunc = 0; b.at = 0; w(&b, "\"theatre\":\"0x%llx\",\"fronts\":[]", (unsigned long long)theatre); }
    w(&b, ",\"truncated\":%s", trunc ? "true" : "false");
    return nfr;
}

int fronts_has_section(const fronts_facts *f, orders_rd rd, uint64_t vt_front, uint64_t vt_section, uint64_t theatre, uint64_t front,
                       int32_t section) {
    uint64_t fd = 0; int32_t fn = 0;
    if (!f->ok || !arr(rd, theatre, f->theatre_fronts_off, f->theatre_fronts_cnt, &fd, &fn)) return 0;
    for (int32_t i = 0; i < fn && i < 64; i++) {
        uint64_t fr = 0, v = 0, sd = 0; int32_t sn = 0;
        if (!rd(fd + 8 * (uint64_t)i, &fr, 8) || fr != front) continue;
        if (!rd(fr, &v, 8) || v != vt_front || !arr(rd, fr, f->front_sections_off, f->front_sections_cnt, &sd, &sn)) return 0;
        for (int32_t k = 0; k < sn && k < 64; k++) {
            uint64_t s = 0, pd = 0; int32_t id = 0, pn = 0;
            if (rd(sd + 8 * (uint64_t)k, &s, 8) && section_ok(f, rd, vt_section, s, &id, &pd, &pn) && id == section) return pn > 0;
        }
        return 0;
    }
    return 0;
}

/* the id of the order of `group` (any list, any depth <= 4) that holds division `div`, 0 when none, -1 on a read fault */
static int32_t holds_in(const orders_facts *f, orders_rd rd, uint64_t vt, uint64_t o, uint64_t div, int depth) {
    uint64_t v = 0, dd = 0, cd = 0; int32_t dn = 0, cn = 0, id = 0;
    if (!rd(o, &v, 8) || v != vt || !rd(o + f->id_off, &id, 4) || !arr(rd, o, f->divs_off, f->divs_cnt, &dd, &dn)) return -1;
    for (int32_t e = 0; e < dn && e < 1024; e++) { uint64_t x = 0; if (!rd(dd + 8 * (uint64_t)e, &x, 8)) return -1; if (x == div) return id; }
    if (depth >= 4 || !arr(rd, o, f->child_off[0], f->child_cnt[0], &cd, &cn)) return depth >= 4 ? 0 : -1;
    for (int32_t c = 0; c < cn && c < 64; c++) {
        uint64_t ch = 0; int32_t r;
        if (!rd(cd + 8 * (uint64_t)c, &ch, 8)) return -1;
        if ((r = holds_in(f, rd, vt, ch, div, depth + 1)) != 0) return r;
    }
    return 0;
}
int32_t orders_holder(const orders_facts *f, orders_rd rd, uint64_t vt, uint64_t group, uint64_t div) {
    for (int l = 0; l < 3; l++) {
        uint64_t data = 0; int32_t size = 0, r;
        if (!arr(rd, group, f->list_off[l], f->list_cnt[l], &data, &size)) return -1;
        for (int32_t i = 0; i < size && i < 64; i++) {
            uint64_t o = 0;
            if (!rd(data + 8 * (uint64_t)i, &o, 8)) return -1;
            if ((r = holds_in(f, rd, vt, o, div, 0)) != 0) return r;
        }
    }
    return 0;
}
/* the top-list order of `group` bound to (front handle, section): its id, 0 when none */
int32_t orders_root_on(const orders_facts *of, const fronts_facts *ff, orders_rd rd, uint64_t vt, uint64_t group, uint64_t front_handle,
                       int32_t section) {
    uint64_t data = 0; int32_t size = 0;
    if (!arr(rd, group, of->list_off[0], of->list_cnt[0], &data, &size)) return 0;
    for (int32_t i = 0; i < size && i < 64; i++) {
        uint64_t o = 0, v = 0, h = 0; int32_t s = 0, id = 0;
        if (rd(data + 8 * (uint64_t)i, &o, 8) && rd(o, &v, 8) && v == vt && rd(o + ff->root_front_off, &h, 8) && h == front_handle &&
            rd(o + ff->root_section_off, &s, 4) && s == section && rd(o + of->id_off, &id, 4))
            return id;
    }
    return 0;
}
