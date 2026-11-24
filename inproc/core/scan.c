/* HOI4 Live Tools in-process module: the scanner core. See scan.h. Pure computation over an image view. */
#include "scan.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define LC_SEGMENT_64 0x19u
#define LC_SYMTAB 0x2u
#define LC_UUID 0x1bu
#define LC_FUNCTION_STARTS 0x26u

static uint32_t rd32(const uint8_t *p) { uint32_t v; memcpy(&v, p, 4); return v; }
static uint64_t rd64(const uint8_t *p) { uint64_t v; memcpy(&v, p, 8); return v; }
static int32_t rds32(const uint8_t *p) { int32_t v; memcpy(&v, p, 4); return v; }

const uint8_t *scan_ptr(const scan_image *im, uint64_t vmaddr, size_t len) {
    for (int i = 0; i < im->nsegs; i++) {
        const scan_seg *s = &im->segs[i];
        if (s->filesize == 0 || vmaddr < s->vmaddr || vmaddr + len < vmaddr) continue;
        if (vmaddr + len > s->vmaddr + s->filesize) continue;
        uint64_t off = s->fileoff + (vmaddr - s->vmaddr);
        if (im->file) {
            if (off + len > im->file_size) return NULL;
            return im->file + off;
        }
        return (const uint8_t *)(uintptr_t)(vmaddr + (uint64_t)im->slide);
    }
    return NULL;
}

int scan_q(const scan_image *im, uint64_t vmaddr, uint64_t *out) {
    const uint8_t *p = scan_ptr(im, vmaddr, 8);
    if (!p) return 0;
    *out = rd64(p);
    return 1;
}

/* bytes of __LINKEDIT data given a file offset */
static const uint8_t *linkedit(const scan_image *im, uint64_t fileoff, size_t len) {
    if (im->file) return (fileoff + len <= im->file_size) ? im->file + fileoff : NULL;
    for (int i = 0; i < im->nsegs; i++) {
        const scan_seg *s = &im->segs[i];
        if (fileoff >= s->fileoff && fileoff + len <= s->fileoff + s->filesize)
            return scan_ptr(im, s->vmaddr + (fileoff - s->fileoff), len);
    }
    return NULL;
}

static int parse(scan_image *im, const uint8_t *h, size_t avail) {
    if (avail < 32 || rd32(h) != 0xfeedfacfu || rd32(h + 4) != 0x01000007u) return 0; /* MH_MAGIC_64, x86_64 */
    uint32_t ncmds = rd32(h + 16), sizeofcmds = rd32(h + 20);
    if (sizeofcmds > (1u << 20) || 32u + sizeofcmds > avail || ncmds > 512) return 0;
    const uint8_t *p = h + 32, *end = h + 32 + sizeofcmds;
    for (uint32_t c = 0; c < ncmds; c++) {
        if (p + 8 > end) return 0;
        uint32_t cmd = rd32(p), size = rd32(p + 4);
        if (size < 8 || p + size > end) return 0;
        if (cmd == LC_SEGMENT_64 && size >= 72 && im->nsegs < SCAN_MAX_SEGS) {
            scan_seg *s = &im->segs[im->nsegs++];
            memcpy(s->name, p + 8, 16); s->name[16] = 0;
            s->vmaddr = rd64(p + 24); s->vmsize = rd64(p + 32); s->fileoff = rd64(p + 40); s->filesize = rd64(p + 48);
            uint32_t nsects = rd32(p + 64);
            if (72u + 80u * nsects > size) return 0;
            if (!strcmp(s->name, "__DATA")) { im->data_lo = s->vmaddr; im->data_hi = s->vmaddr + s->vmsize; }
            for (uint32_t k = 0; k < nsects; k++) {
                const uint8_t *q = p + 72 + 80 * k;
                char sect[17], seg[17];
                memcpy(sect, q, 16); sect[16] = 0; memcpy(seg, q + 16, 16); seg[16] = 0;
                uint64_t a = rd64(q + 32), z = rd64(q + 40);
                if (!strcmp(seg, "__TEXT") && !strcmp(sect, "__text")) { im->text_addr = a; im->text_size = z; }
                if (!strcmp(seg, "__TEXT") && !strcmp(sect, "__cstring")) { im->cstr_addr = a; im->cstr_size = z; }
                if (!strcmp(seg, "__TEXT") && !strcmp(sect, "__const")) { im->tconst_addr = a; im->tconst_size = z; }
                if (!strcmp(seg, "__DATA") && !strcmp(sect, "__const")) { im->dconst_addr = a; im->dconst_size = z; }
            }
        } else if (cmd == LC_SYMTAB && size >= 24) {
            im->symoff = rd32(p + 8); im->nsyms = rd32(p + 12); im->stroff = rd32(p + 16); im->strsize = rd32(p + 20);
        } else if (cmd == LC_FUNCTION_STARTS && size >= 16) {
            im->fstarts_off = rd32(p + 8); im->fstarts_size = rd32(p + 12);
        } else if (cmd == LC_UUID && size >= 24) {
            const uint8_t *u = p + 8;
            snprintf(im->uuid, sizeof im->uuid,
                     "%02X%02X%02X%02X-%02X%02X-%02X%02X-%02X%02X-%02X%02X%02X%02X%02X%02X", u[0], u[1], u[2], u[3],
                     u[4], u[5], u[6], u[7], u[8], u[9], u[10], u[11], u[12], u[13], u[14], u[15]);
        }
        p += size;
    }
    if (!im->text_size || !im->cstr_size || !im->dconst_size || !im->fstarts_size) return 0;
    /* function starts: ULEB128 deltas from the __TEXT segment's vm address */
    uint64_t text_seg = 0;
    for (int i = 0; i < im->nsegs; i++) if (!strcmp(im->segs[i].name, "__TEXT")) text_seg = im->segs[i].vmaddr;
    const uint8_t *b = linkedit(im, im->fstarts_off, im->fstarts_size);
    if (!b) return 0;
    size_t cap = im->fstarts_size; /* at most one start per byte */
    im->fstarts = (uint64_t *)malloc(cap * sizeof(uint64_t));
    if (!im->fstarts) return 0;
    uint64_t a = text_seg;
    for (size_t i = 0; i < im->fstarts_size;) {
        uint64_t v = 0; unsigned sh = 0; uint8_t x;
        do { x = b[i++]; v |= (uint64_t)(x & 0x7f) << sh; sh += 7; } while ((x & 0x80) && i < im->fstarts_size && sh < 64);
        if (v == 0) break;
        a += v; im->fstarts[im->nfstarts++] = a;
    }
    return im->nfstarts > 0;
}

int scan_image_open_file(scan_image *im, const uint8_t *file, size_t size) {
    memset(im, 0, sizeof *im);
    im->file = file; im->file_size = size;
    return parse(im, file, size);
}

int scan_image_open_process(scan_image *im, const void *header, int64_t slide) {
    memset(im, 0, sizeof *im);
    im->slide = slide;
    /* the header and load commands lie inside the mapped __TEXT segment; 64 KiB bounds the parse */
    return parse(im, (const uint8_t *)header, 1u << 16);
}

void scan_image_free(scan_image *im) { free(im->fstarts); im->fstarts = NULL; im->nfstarts = 0; }

static uint64_t func_of(const scan_image *im, uint64_t a) {
    size_t lo = 0, hi = im->nfstarts;
    if (!hi || a < im->fstarts[0]) return 0;
    while (hi - lo > 1) { size_t mid = (lo + hi) / 2; if (im->fstarts[mid] <= a) lo = mid; else hi = mid; }
    return im->fstarts[lo];
}

/* exact NUL-terminated string in a section, preceded by NUL; returns its vm address or 0 */
static uint64_t find_string(const scan_image *im, uint64_t sa, uint64_t sz, const char *s) {
    const uint8_t *b = scan_ptr(im, sa, sz);
    size_t n = strlen(s) + 1;
    if (!b || sz < n) return 0;
    for (uint64_t i = 0; i + n <= sz; i++) {
        if (b[i] != (uint8_t)s[0] || memcmp(b + i, s, n)) continue;
        if (i == 0 || b[i - 1] == 0) return sa + i;
    }
    return 0;
}

/* RIP-relative lea/mov r64,[rip+d32] at text index i: returns 1 and the target */
static int riprel(const uint8_t *t, uint64_t i, uint64_t n, uint64_t va, int want_lea, uint64_t *target) {
    if (i + 7 > n) return 0;
    if ((t[i] != 0x48 && t[i] != 0x4c) || (t[i + 2] & 0xc7) != 0x05) return 0;
    if (want_lea == 1 && t[i + 1] != 0x8d) return 0;
    if (want_lea == 0 && t[i + 1] != 0x8b) return 0;
    if (want_lea == 2 && t[i + 1] != 0x8d && t[i + 1] != 0x8b) return 0;
    *target = va + i + 7 + (int64_t)rds32(t + i + 3);
    return 1;
}

static void cap_init(scan_cap *c, const char *name, const char *source) {
    memset(c, 0, sizeof *c); c->name = name; snprintf(c->source, sizeof c->source, "%s", source);
}

int scan_post_function(const scan_image *im, scan_cap *cap) {
    cap_init(cap, "command_post", "scan");
    const char *needle = "Dropped command: ";
    uint64_t s = find_string(im, im->cstr_addr, im->cstr_size, needle);
    if (!s) { snprintf(cap->evidence, sizeof cap->evidence, "string 'Dropped command: ' absent"); return 0; }
    const uint8_t *t = scan_ptr(im, im->text_addr, im->text_size);
    if (!t) return 0;
    uint64_t n = im->text_size, va = im->text_addr, cand[8]; unsigned nc = 0, calls[8] = {0};
    for (uint64_t i = 0; i + 7 <= n; i++) {
        uint64_t tg;
        if (riprel(t, i, n, va, 1, &tg) && tg == s) {
            uint64_t f = func_of(im, va + i); unsigned k;
            for (k = 0; k < nc && cand[k] != f; k++) {}
            if (k == nc && nc < 8) cand[nc++] = f;
        }
    }
    if (!nc) { snprintf(cap->evidence, sizeof cap->evidence, "no code references the string"); return 0; }
    for (uint64_t i = 0; i + 5 <= n; i++) {
        if (t[i] != 0xe8) continue;
        uint64_t tg = va + i + 5 + (int64_t)rds32(t + i + 1);
        for (unsigned k = 0; k < nc; k++) if (tg == cand[k]) calls[k]++;
    }
    unsigned best = 0, second = 0;
    for (unsigned k = 1; k < nc; k++) if (calls[k] > calls[best]) best = k;
    for (unsigned k = 0; k < nc; k++) if (k != best && calls[k] > second) second = calls[k];
    cap->value = cand[best]; cap->extra = calls[best];
    cap->ok = calls[best] >= 20 && calls[best] > 4 * second;
    snprintf(cap->evidence, sizeof cap->evidence,
             "string at 0x%llx used by %u functions; chosen has %u direct callers (next best %u)",
             (unsigned long long)s, nc, calls[best], second);
    return cap->ok;
}

int scan_queue_root(const scan_image *im, uint64_t post, scan_cap *cap) {
    cap_init(cap, "queue_root", "scan");
    const uint8_t *t = scan_ptr(im, im->text_addr, im->text_size);
    if (!t || !post) return 0;
    uint64_t n = im->text_size, va = im->text_addr;
    uint64_t g[32], sl[32]; unsigned v[32], ng = 0, sites = 0;
    for (uint64_t i = 40; i + 5 <= n; i++) {
        if (t[i] != 0xe8 || va + i + 5 + (int64_t)rds32(t + i + 1) != post) continue;
        sites++;
        /* mov rdi,[rip+G] ; mov rax,[rdi] ; call [rax+slot] ; ... ; call post */
        for (uint64_t p = i - 40; p + 16 <= i; p++) {
            if (t[p] != 0x48 || t[p + 1] != 0x8b || t[p + 2] != 0x3d) continue;
            if (t[p + 7] != 0x48 || t[p + 8] != 0x8b || t[p + 9] != 0x07 || t[p + 10] != 0xff) continue;
            uint64_t slot;
            if (t[p + 11] == 0x90) slot = (uint64_t)rd32(t + p + 12);
            else if (t[p + 11] == 0x50) slot = t[p + 12];
            else continue;
            uint64_t G = va + p + 7 + (int64_t)rds32(t + p + 3);
            unsigned k;
            for (k = 0; k < ng && !(g[k] == G && sl[k] == slot); k++) {}
            if (k == ng && ng < 32) { g[ng] = G; sl[ng] = slot; v[ng++] = 0; }
            if (k < 32) v[k]++;
        }
    }
    if (!ng) { snprintf(cap->evidence, sizeof cap->evidence, "%u post call sites, no root load pattern", sites); return 0; }
    unsigned b = 0;
    for (unsigned k = 1; k < ng; k++) if (v[k] > v[b]) b = k;
    cap->value = g[b]; cap->extra = sl[b];
    cap->ok = v[b] >= 2 && g[b] >= im->data_lo && g[b] < im->data_hi;
    snprintf(cap->evidence, sizeof cap->evidence, "%u post call sites; root global voted %u times of %u patterns",
             sites, v[b], ng);
    return cap->ok;
}

static int rtti_name_is(const scan_image *im, uint64_t vt, const char *mangled) {
    uint64_t ti, name, top;
    if (!scan_q(im, vt - 16, &top) || top != 0 || !scan_q(im, vt - 8, &ti) || !scan_q(im, ti + 8, &name)) return 0;
    size_t n = strlen(mangled) + 1;
    const uint8_t *s = scan_ptr(im, name, n);
    return s && !memcmp(s, mangled, n);
}

int scan_command_factory(const scan_image *im, const char *cls, scan_cap *factory, scan_cap *vtable,
                         scan_cap *type_id) {
    char sym[256], mangled[160];
    cap_init(factory, "factory", "export"); cap_init(vtable, "vtable", "rtti"); cap_init(type_id, "type_id", "scan");
    snprintf(mangled, sizeof mangled, "%zu%s", strlen(cls), cls);
    snprintf(sym, sizeof sym, "__ZN14CCommandHelperI%sE18CommandRegistrator6CreateEv", mangled);
    const uint8_t *syms = linkedit(im, im->symoff, (size_t)im->nsyms * 16);
    const uint8_t *strs = linkedit(im, im->stroff, im->strsize);
    if (!syms || !strs) { snprintf(factory->evidence, sizeof factory->evidence, "no symbol table"); return 0; }
    size_t sl = strlen(sym) + 1;
    for (uint32_t k = 0; k < im->nsyms; k++) {
        const uint8_t *e = syms + 16 * (size_t)k;
        uint32_t strx = rd32(e); uint8_t type = e[4];
        if ((type & 0xe0) || (type & 0x0e) != 0x0e || strx + sl > im->strsize) continue;
        if (!memcmp(strs + strx, sym, sl)) { factory->value = rd64(e + 8); break; }
    }
    if (!factory->value) { snprintf(factory->evidence, sizeof factory->evidence, "symbol %s absent", sym + 1); return 0; }
    const uint8_t *f = scan_ptr(im, factory->value, 0xa0);
    if (!f) return 0;
    for (unsigned i = 0; i + 7 <= 0xa0; i++) {
        uint64_t tg;
        if (!factory->extra && f[i] == 0xbf) factory->extra = rd32(f + i + 1); /* mov edi, size (operator new) */
        if (riprel(f, i, 0xa0, factory->value, 1, &tg) && tg >= im->dconst_addr &&
            tg < im->dconst_addr + im->dconst_size && rtti_name_is(im, tg, mangled)) { vtable->value = tg; break; }
        /* engineer 76: lea rax, [vtable object]; add rax, 0x10; mov [rbx], rax (CSelectEventOptionCommand's factory
         * addresses the vtable object, then steps over offset-to-top and the typeinfo to its address point) */
        if (riprel(f, i, 0xa0, factory->value, 1, &tg) && i + 11 <= 0xa0 && f[i + 7] == 0x48 && f[i + 8] == 0x83 &&
            f[i + 9] == 0xc0 && f[i + 10] == 0x10 && tg + 0x10 >= im->dconst_addr && tg + 0x10 < im->dconst_addr + im->dconst_size &&
            rtti_name_is(im, tg + 0x10, mangled)) { vtable->value = tg + 0x10; break; }
    }
    /* engineer 75: a factory that calls a separate default constructor (CArmyGroupCommand: operator new, then call
     * ctor): the vtable store is in that constructor; follow the first direct call after the allocation */
    for (unsigned i = 0; !vtable->value && i + 5 <= 0x60; i++) {
        if (f[i] != 0xe8 || i < 8) continue;
        uint64_t callee = factory->value + i + 5 + (int64_t)(int32_t)rd32(f + i + 1);
        const uint8_t *cb = (callee >= im->text_addr && callee < im->text_addr + im->text_size) ? scan_ptr(im, callee, 0x80) : NULL;
        if (!cb || cb[0] != 0x55) continue; /* operator new is a stub, not a frame */
        for (unsigned k = 0; k + 7 <= 0x80; k++) {
            uint64_t tg;
            if (riprel(cb, k, 0x80, callee, 1, &tg) && tg >= im->dconst_addr && tg < im->dconst_addr + im->dconst_size &&
                rtti_name_is(im, tg, mangled)) { vtable->value = tg; break; }
        }
        break;
    }
    factory->ok = 1;
    snprintf(factory->evidence, sizeof factory->evidence, "symbol table entry %.120s", sym + 1);
    if (!vtable->value) { snprintf(vtable->evidence, sizeof vtable->evidence, "no RTTI-verified vtable store"); return 0; }
    vtable->ok = 1;
    unsigned slots = 0, hits = 0;
    for (; slots < 64; slots++) {
        uint64_t fn;
        if (!scan_q(im, vtable->value + 8 * slots, &fn) || fn < im->text_addr || fn >= im->text_addr + im->text_size) break;
        const uint8_t *b = scan_ptr(im, fn, 11);
        static const uint8_t pre[5] = {0x55, 0x48, 0x89, 0xe5, 0xb8};
        if (b && !memcmp(b, pre, 5) && b[9] == 0x5d && b[10] == 0xc3) {
            hits++; type_id->value = rd32(b + 5); type_id->extra = slots;
        }
    }
    vtable->extra = slots;
    snprintf(vtable->evidence, sizeof vtable->evidence, "store in factory; typeinfo name %s; %u slots", mangled, slots);
    type_id->ok = hits == 1;
    snprintf(type_id->evidence, sizeof type_id->evidence, "%u constant-returning slots", hits);
    return type_id->ok;
}

int scan_class_vtable(const scan_image *im, const char *mangled, scan_cap *cap) {
    cap_init(cap, mangled, "rtti");
    uint64_t name = find_string(im, im->tconst_addr, im->tconst_size, mangled);
    if (!name) name = find_string(im, im->cstr_addr, im->cstr_size, mangled);
    const uint8_t *d = scan_ptr(im, im->dconst_addr, im->dconst_size);
    if (!d) return 0;
    uint64_t ti = 0, vt = 0; unsigned nti = 0, nvt = 0;
    if (!name) {
        /* engineer 76: a type name packed right after other constant data (12CEventWindow follows a float in
         * __TEXT,__const): every occurrence of the exact name, kept only when exactly one typeinfo points at it */
        const uint8_t *b = scan_ptr(im, im->tconst_addr, im->tconst_size);
        size_t ln = strlen(mangled) + 1; unsigned hits = 0;
        for (uint64_t i = 0; b && i + ln <= im->tconst_size; i++) {
            if (b[i] != (uint8_t)mangled[0] || memcmp(b + i, mangled, ln)) continue;
            unsigned refs = 0;
            for (uint64_t k = 8; k + 8 <= im->dconst_size; k += 8) if (rd64(d + k) == im->tconst_addr + i) refs++;
            if (refs == 1) { name = im->tconst_addr + i; hits++; }
        }
        if (hits != 1) name = 0;
    }
    if (!name) { snprintf(cap->evidence, sizeof cap->evidence, "type name absent"); return 0; }
    for (uint64_t i = 8; i + 8 <= im->dconst_size; i += 8)
        if (rd64(d + i) == name) { ti = im->dconst_addr + i - 8; nti++; }
    if (nti == 1)
        for (uint64_t i = 8; i + 8 <= im->dconst_size; i += 8)
            if (rd64(d + i) == ti && rd64(d + i - 8) == 0) { vt = im->dconst_addr + i + 8; nvt++; }
    cap->value = vt; cap->ok = nti == 1 && nvt == 1;
    snprintf(cap->evidence, sizeof cap->evidence, "name at 0x%llx, %u typeinfo, %u primary vtables",
             (unsigned long long)name, nti, nvt);
    return cap->ok;
}

static int cmp64(const void *a, const void *b) {
    uint64_t x = *(const uint64_t *)a, y = *(const uint64_t *)b;
    return x < y ? -1 : x > y;
}

size_t scan_global_candidates(const scan_image *im, unsigned min_refs, uint64_t **out, unsigned **refs) {
    *out = NULL; *refs = NULL;
    const uint8_t *t = scan_ptr(im, im->text_addr, im->text_size);
    if (!t) return 0;
    size_t cap = 1 << 20, n = 0;
    uint64_t *all = (uint64_t *)malloc(cap * sizeof *all);
    if (!all) return 0;
    for (uint64_t i = 0; i + 7 <= im->text_size; i++) {
        uint64_t tg;
        if (!riprel(t, i, im->text_size, im->text_addr, 2, &tg) || tg < im->data_lo || tg >= im->data_hi) continue;
        if (n == cap) {
            uint64_t *grown = (uint64_t *)realloc(all, 2 * cap * sizeof *all);
            if (!grown) break;
            all = grown; cap *= 2;
        }
        all[n++] = tg;
    }
    qsort(all, n, sizeof *all, cmp64);
    size_t m = 0;
    for (size_t i = 0; i < n;) {
        size_t j = i;
        while (j < n && all[j] == all[i]) j++;
        if (j - i >= min_refs) m++;
        i = j;
    }
    uint64_t *g = (uint64_t *)malloc((m ? m : 1) * sizeof *g);
    unsigned *r = (unsigned *)malloc((m ? m : 1) * sizeof *r);
    if (!g || !r) { free(all); free(g); free(r); return 0; }
    size_t k = 0;
    for (size_t i = 0; i < n;) {
        size_t j = i;
        while (j < n && all[j] == all[i]) j++;
        if (j - i >= min_refs) { g[k] = all[i]; r[k] = (unsigned)(j - i); k++; }
        i = j;
    }
    free(all);
    for (size_t a = 1; a < k; a++) /* most referenced first (insertion sort; k is small) */
        for (size_t b = a; b > 0 && r[b] > r[b - 1]; b--) {
            unsigned tr = r[b]; r[b] = r[b - 1]; r[b - 1] = tr;
            uint64_t tg = g[b]; g[b] = g[b - 1]; g[b - 1] = tg;
        }
    *out = g; *refs = r;
    return k;
}

const char *scan_cap_json(const scan_cap *c, char *buf, size_t n) {
    snprintf(buf, n, "{\"name\":\"%s\",\"ok\":%s,\"value\":\"0x%llx\",\"extra\":\"0x%llx\",\"source\":\"%s\","
                     "\"evidence\":\"%s\"}",
             c->name, c->ok ? "true" : "false", (unsigned long long)c->value, (unsigned long long)c->extra, c->source,
             c->evidence);
    return buf;
}
