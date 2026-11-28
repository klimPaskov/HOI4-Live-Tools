/* engineer 84: the organisations reader (core/mio.c) on a synthetic image laid out with the scanned facts (1.20.0.0 and
 * 1.19.4.1 share them). Checks: the own list only, every field, the tree order and each trait's state, the libc++
 * std::set walk, both std::string forms, the policy gate per organisation, the post helpers, a too-small buffer. */
#include "../core/mio.h"
#include <stdio.h>
#include <string.h>

#define BASE 0x200000000ull
static uint8_t mem[0x10000];
static uint64_t top = BASE + 0x10;
static int fails;
static int rd(uint64_t a, void *b, size_t n) {
    if (a < BASE || a + n > BASE + sizeof mem) return 0;
    memcpy(b, mem + (a - BASE), n); return 1;
}
static uint64_t alloc(size_t n) { uint64_t a = top; top += (n + 15) & ~(size_t)15; return a; }
static void w64(uint64_t a, uint64_t v) { memcpy(mem + (a - BASE), &v, 8); }
static void w32(uint64_t a, int32_t v) { memcpy(mem + (a - BASE), &v, 4); }
static void w8(uint64_t a, uint8_t v) { mem[a - BASE] = v; }
static void check(const char *what, int ok) { printf("%s %s\n", ok ? "ok  " : "FAIL", what); if (!ok) fails++; }

static uint64_t names[64]; /* token - 100 -> std::string address */
static uint64_t mkstr(const char *s) {
    uint64_t a = alloc(24); size_t n = strlen(s);
    if (n <= 22) { w8(a, (uint8_t)(n << 1)); memcpy(mem + (a + 1 - BASE), s, n); }
    else { uint64_t d = alloc(n + 1); memcpy(mem + (d - BASE), s, n); w8(a, 1); w64(a + 8, n); w64(a + 16, d); }
    return a;
}
static uint64_t name_cb(void *c, int32_t t) { (void)c; return t >= 100 && t < 164 ? names[t - 100] : 0; }
static int can_cb(void *c, uint64_t org, uint64_t tr) { (void)c; (void)org; int32_t t = 0; rd(tr + 8, &t, 4); return t == 113; }
static int pol_cb(void *c, uint64_t org, uint64_t h, int32_t p) { (void)c; (void)org; return h == 0x7700000001ull && p == 140; }

int main(void) {
    mio_facts f = {0};
    f.ok = 1; f.mgr_off = 0xd10; f.orgs_off = 0x130; f.orgs_cnt = 0x13c; f.pol_off = 0x148; f.pol_cnt = 0x154;
    f.tag_off = 0x100; f.policy_off = 0x170; f.key_off = 0x38; f.funds_off = 0x108; f.size_off = 0x110; f.points_off = 0x114;
    f.unlocked_off = 0x120; f.queue_off = 0x138; f.queue_cnt = 0x144; f.tree_off = 0xf0; f.tree_arr = 0x28; f.tree_cnt = 0x34;
    f.trait_stride = 0x380; f.empty_tok = 0x4c17;
    const uint64_t VT = 0x1035913d0ull;
    names[0] = mkstr("ita_ansaldo_long_organisation_key"); names[11] = mkstr("trait_a"); names[12] = mkstr("trait_b");
    names[13] = mkstr("trait_c"); names[14] = mkstr("trait_d"); names[40] = mkstr("pol_x"); names[41] = mkstr("pol_y");
    /* the tree: 4 templates, tokens 111..114 */
    uint64_t tree = alloc(0x40), tarr = alloc(4 * 0x380);
    w64(tree + 0x28, tarr); w32(tree + 0x34, 4);
    for (int i = 0; i < 4; i++) w32(tarr + (uint64_t)i * 0x380 + 8, 111 + i);
    /* the own organisation */
    uint64_t org = alloc(0x200), org2 = alloc(0x200);
    w64(org, VT); w64(org + 8, 0x7700000001ull); w32(org + 0x38, 100); w64(org + 0x108, 2500000); w32(org + 0x110, 3);
    w32(org + 0x114, 7); w32(org + 0x170, 141); w64(org + 0xf0, tree); w32(org + 0x100, 5);
    /* the queue: trait 112 */
    uint64_t qa = alloc(0x20); w32(qa + 8, 112); w64(org + 0x138, qa); w32(org + 0x144, 1);
    /* the unlocked set: root 111 with children 114 (left) and 0 (right): 2 nodes */
    uint64_t n1 = alloc(0x30), n2 = alloc(0x30);
    w64(n1, n2); w64(n1 + 8, 0); w32(n1 + 0x28, 111); w64(n2, 0); w64(n2 + 8, 0); w32(n2 + 0x28, 114);
    w64(org + 0x120, n2); w64(org + 0x128, n1); w64(org + 0x130, 2);
    /* another country's organisation in the list (a manager never holds one, but the reader must skip it) */
    w64(org2, VT); w64(org2 + 8, 0x7700000002ull); w32(org2 + 0x100, 9);
    /* the manager and the policies */
    uint64_t mgr = alloc(0x200), oarr = alloc(16), parr = alloc(16), p1 = alloc(0x50), p2 = alloc(0x50);
    w64(oarr, org); w64(oarr + 8, org2); w64(mgr + 0x130, oarr); w32(mgr + 0x13c, 2);
    w32(p1 + 8, 140); w32(p1 + 0x40, 150); w32(p2 + 8, 141); w32(p2 + 0x40, 100);
    w64(parr, p1); w64(parr + 8, p2); w64(mgr + 0x148, parr); w32(mgr + 0x154, 2);

    mio_env e = {rd, name_cb, can_cb, pol_cb, NULL, VT};
    char out[4096];
    int n = mio_json(&f, &e, mgr, 5, out, sizeof out);
    printf("%s\n", out);
    check("one own organisation read (the other country's skipped)", n == 1 && strstr(out, "\"organisations_read\":1"));
    check("key (long std::string), funds, size, points, attached policy and its key",
          strstr(out, "\"key\":\"ita_ansaldo_long_organisation_key\",\"funds_raw\":2500000,\"size\":3,\"trait_points\":7,\"policy\":141,\"policy_key\":\"pol_y\"") != NULL);
    check("policies with their keys and costs", strstr(out, "\"policies\":[{\"token\":140,\"key\":\"pol_x\",\"cost\":150},{\"token\":141,\"key\":\"pol_y\",\"cost\":100}]") != NULL);
    check("the gate per organisation", strstr(out, "\"policies_allowed\":[140]") != NULL);
    check("tree order with states u/q/a/l", strstr(out, "\"queue\":[112],\"unlocked\":2,\"traits\":[[111,\"trait_a\",\"u\"],[112,\"trait_b\",\"q\"],[113,\"trait_c\",\"a\"],[114,\"trait_d\",\"u\"]]") != NULL);
    check("not truncated", strstr(out, "\"truncated\":false") != NULL);
    check("helpers: in list / not", mio_in_list(&f, rd, mgr, org) == 1 && mio_in_list(&f, rd, mgr, mgr) == 0);
    check("helpers: trait in tree / not", mio_trait(&f, rd, org, 113) == tarr + 2 * 0x380 && mio_trait(&f, rd, org, 999) == 0);
    check("helpers: country policy / not", mio_policy(&f, rd, mgr, 141) == p2 && mio_policy(&f, rd, mgr, 0x4c17) == 0);
    check("helpers: queued", mio_queued(&f, rd, org, 112) == 1 && mio_queued(&f, rd, org, 111) == 0);
    check("helpers: unlocked", mio_unlocked(&f, rd, org, 114) == 1 && mio_unlocked(&f, rd, org, 113) == 0);
    w64(org + 0x130, 3); /* the set's size disagrees with its nodes: nothing guessed */
    check("a set whose size disagrees is unreadable", mio_unlocked(&f, rd, org, 114) == -1 && mio_json(&f, &e, mgr, 5, out, sizeof out) == -1);
    w64(org + 0x130, 2);
    char small[200];
    check("a buffer too small is refused, never cut mid-object", mio_json(&f, &e, mgr, 5, small, sizeof small) == -1);
    w64(org2, 0x1234); /* an object of another class in the list */
    check("an object of another class stops the read", mio_json(&f, &e, mgr, 5, out, sizeof out) == -1);
    printf(fails ? "MIO FAILED %d\n" : "MIO ALL PASS\n", fails);
    return fails != 0;
}
