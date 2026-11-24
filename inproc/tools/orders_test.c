/* orders_test: the battle-plan order reader against a structure laid out with the facts the scan reads from a real
 * executable (engineer 73). Usage: orders_test EXE. Builds, in a private buffer, an army whose plan holds one front
 * (3 provinces, 4 divisions) with one offensive arrow (2 divisions) and its target line, plus one fallback line, then
 * checks the JSON tree, the lookup by id, and the divisions a new front under the front would receive. */
#include "../core/orders.h"
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

static uint8_t mem[0x20000];
#define BASE 0x500000000ULL
static int rd(uint64_t a, void *b, size_t n) {
    if (a < BASE || a + n > BASE + sizeof mem) return 0;
    memcpy(b, mem + (a - BASE), n);
    return 1;
}
static void put(uint64_t a, const void *v, size_t n) { memcpy(mem + (a - BASE), v, n); }
static void q(uint64_t a, uint64_t v) { put(a, &v, 8); }
static void d(uint64_t a, int32_t v) { put(a, &v, 4); }
static void array(uint64_t obj, uint32_t off, uint32_t cnt, uint64_t data, int32_t size) { q(obj + off, data); d(obj + off + 8, size); d(obj + cnt, size); }

static const orders_facts *F;
static uint64_t VT = 0x1234560;
static uint64_t order(uint64_t at, int kind, int id, int exec, uint64_t group, const int32_t *provs, int np, uint64_t provs_at,
                      const uint64_t *divs, int nd, uint64_t divs_at) {
    q(at, VT); d(at + F->kind_off, kind); d(at + F->id_off, id); mem[at - BASE + F->exec_off] = (uint8_t)exec;
    q(at + F->owner_off, group);
    for (int i = 0; i < np; i++) d(provs_at + 4 * (uint64_t)i, provs[i]);
    array(at, F->provs_off, F->provs_cnt, provs_at, np);
    for (int i = 0; i < nd; i++) q(divs_at + 8 * (uint64_t)i, divs[i]);
    array(at, F->divs_off, F->divs_cnt, divs_at, nd);
    array(at, F->child_off[0], F->child_cnt[0], 0, 0);
    array(at, F->child_off[1], F->child_cnt[1], 0, 0);
    return at;
}

int main(int argc, char **argv) {
    if (argc < 2) return 2;
    int fd = open(argv[1], O_RDONLY); struct stat st;
    if (fd < 0 || fstat(fd, &st)) return 2;
    const uint8_t *map = mmap(NULL, (size_t)st.st_size, PROT_READ, MAP_PRIVATE, fd, 0);
    scan_image im;
    if (map == MAP_FAILED || !scan_image_open_file(&im, map, (size_t)st.st_size)) return 2;
    orders_facts f;
    if (!scan_orders_facts(&im, &f)) { printf("FAIL facts: %s\n", f.evidence); return 1; }
    F = &f;
    uint64_t G = BASE + 0x100, FRONT = BASE + 0x1000, ARROW = BASE + 0x2000, TARGET = BASE + 0x3000, FB = BASE + 0x4000;
    uint64_t D[4] = {0x7000001, 0x7000002, 0x7000003, 0x7000004};
    int32_t fp[3] = {631, 632, 661}, ap[2] = {661, 861}, tp[3] = {851, 861, 871}, bp[2] = {101, 102};
    order(FRONT, 2, 190, 1, G, fp, 3, BASE + 0x8000, D, 4, BASE + 0x8100);
    order(ARROW, 1, 193, 1, G, ap, 2, BASE + 0x8200, D, 2, BASE + 0x8300);
    order(TARGET, 2, 192, 0, G, tp, 3, BASE + 0x8400, D, 2, BASE + 0x8500);
    order(FB, 2, 200, 0, G, bp, 2, BASE + 0x8600, NULL, 0, BASE + 0x8700);
    q(BASE + 0x8800, ARROW); array(FRONT, f.child_off[0], f.child_cnt[0], BASE + 0x8800, 1);
    q(BASE + 0x8810, TARGET); array(ARROW, f.child_off[0], f.child_cnt[0], BASE + 0x8810, 1);
    mem[FRONT - BASE + f.blitz_off] = 0; mem[ARROW - BASE + f.blitz_off] = 1; q(ARROW + f.drawer_off, BASE + 0x9999);
    q(BASE + 0x8900, FRONT); array(G, f.list_off[0], f.list_cnt[0], BASE + 0x8900, 1);
    q(BASE + 0x8910, FB); array(G, f.list_off[1], f.list_cnt[1], BASE + 0x8910, 1);
    array(G, f.list_off[2], f.list_cnt[2], 0, 0);
    static char out[65536];
    int bad = 0;
    int n = orders_tree_json(&f, rd, VT, G, out, sizeof out);
#define CHECK(name, cond) do { int ok_ = (cond) ? 1 : 0; bad += !ok_; printf("%s %s\n", ok_ ? "PASS" : "FAIL", name); } while (0)
    CHECK("tree reads 4 orders", n == 4);
    CHECK("front with id, kind, provinces", strstr(out, "\"kind\":2,\"id\":190,\"executing\":true") && strstr(out, "\"provinces\":[631,632,661]"));
    CHECK("arrow is a child of the front, spearhead byte read", strstr(out, "\"kind\":1,\"id\":193,\"executing\":true,\"blitz\":true"));
    CHECK("target line under the arrow", strstr(out, "\"id\":192") && strstr(out, "[851,861,871]"));
    CHECK("fallback list", strstr(out, "{\"list\":\"fallback\",\"orders\":[{") && strstr(out, "\"id\":200"));
    CHECK("divisions counted and listed", strstr(out, "\"divisions\":4,\"division_ptrs\":[\"0x7000001\""));
    CHECK("not truncated", strstr(out, "\"truncated\":false") != NULL);
    CHECK("find by id: nested target", orders_find(&f, rd, VT, G, 192) == TARGET);
    CHECK("find by id: fallback", orders_find(&f, rd, VT, G, 200) == FB);
    CHECK("find by id: missing", orders_find(&f, rd, VT, G, 999) == 0);
    uint64_t fr[8];
    int k = orders_free_divisions(&f, rd, FRONT, fr, 8);
    CHECK("a new front under the front gets the 2 divisions the arrow does not hold", k == 2 && fr[0] == D[2] && fr[1] == D[3]);
    q(ARROW, VT + 8); /* a wrong vtable: nothing guessed */
    CHECK("a wrong vtable fails the read", orders_tree_json(&f, rd, VT, G, out, sizeof out) < 0);
    q(ARROW, VT);
    static char small[300];
    n = orders_tree_json(&f, rd, VT, G, small, sizeof small);
    CHECK("a small buffer says truncated", n == 4 && strstr(small, "\"truncated\":true"));
    /* engineer 74: holders (assign/unassign read-back) */
    CHECK("holder: a division on the arrow is held by the front first (outermost order wins)", orders_holder(&f, rd, VT, G, D[0]) == 190);
    CHECK("holder: a division in no order", orders_holder(&f, rd, VT, G, 0x7000009) == 0);
    /* engineer 74: the fronts a plan root binds to: army -> army group -> theatre -> fronts -> sections -> provinces */
    fronts_facts ff;
    if (!scan_fronts_facts(&im, &ff)) { printf("FAIL front facts: %s\n", ff.evidence); return 1; }
    uint64_t VG = 0x2222220, VTH = 0x3333330, VFR = 0x4444440, VSE = 0x5555550;
    uint64_t AG = BASE + 0xa000, TH = BASE + 0xb000, FR1 = BASE + 0xc000, S1 = BASE + 0xd000, S2 = BASE + 0xd800;
    uint64_t P[3] = {BASE + 0xe000, BASE + 0xe400, BASE + 0xe800};
    q(G + ff.group_parent_off, AG); q(AG, VG); q(AG + ff.group_parent_off, TH); q(TH, VTH);
    q(BASE + 0xf000, FR1); array(TH, ff.theatre_fronts_off, ff.theatre_fronts_cnt, BASE + 0xf000, 1);
    q(FR1, VFR); q(BASE + 0xf100, S1); q(BASE + 0xf108, S2); array(FR1, ff.front_sections_off, ff.front_sections_cnt, BASE + 0xf100, 2);
    q(S1, VSE); d(S1 + ff.section_id_off, 4); q(S2, VSE); d(S2 + ff.section_id_off, 7);
    d(P[0] + ff.province_id_off, 3001); d(P[1] + ff.province_id_off, 3002); d(P[2] + ff.province_id_off, 3003);
    q(BASE + 0xf200, P[0]); q(BASE + 0xf208, P[1]); array(S1, ff.section_provs_off, ff.section_provs_cnt, BASE + 0xf200, 2);
    q(BASE + 0xf300, P[2]); array(S2, ff.section_provs_off, ff.section_provs_cnt, BASE + 0xf300, 1);
    CHECK("theatre found through the army group", fronts_theatre_of(&ff, rd, VG, VTH, G) == TH);
    n = fronts_json(&ff, rd, VFR, VSE, TH, out, sizeof out);
    CHECK("fronts read: one front, two sections with their province ids",
          n == 1 && strstr(out, "{\"id\":4,\"provinces\":[3001,3002]}") && strstr(out, "{\"id\":7,\"provinces\":[3003]}"));
    CHECK("section check: present", fronts_has_section(&ff, rd, VFR, VSE, TH, FR1, 7) == 1);
    CHECK("section check: unknown section", fronts_has_section(&ff, rd, VFR, VSE, TH, FR1, 9) == 0);
    CHECK("section check: a front of another theatre", fronts_has_section(&ff, rd, VFR, VSE, TH, BASE + 0xc800, 4) == 0);
    q(AG, VG + 8);
    CHECK("an unknown parent class: no theatre (nothing guessed)", fronts_theatre_of(&ff, rd, VG, VTH, G) == 0);
    q(AG, VG);
    q(S2, VSE + 8);
    CHECK("a wrong section vtable fails the fronts read", fronts_json(&ff, rd, VFR, VSE, TH, out, sizeof out) < 0);
    q(S2, VSE);
    q(FRONT + ff.root_front_off, 0x0000123400001269ULL); d(FRONT + ff.root_section_off, 7);
    CHECK("the root bound to (front handle, section) is found by the new_root read-back",
          orders_root_on(&f, &ff, rd, VT, G, 0x0000123400001269ULL, 7) == 190 && orders_root_on(&f, &ff, rd, VT, G, 0x0000123400001269ULL, 4) == 0);
    printf(bad ? "ORDERS FAILED\n" : "ORDERS ALL PASS\n");
    return bad ? 1 : 0;
}
