/* HOI4 Live Tools in-process module: the BATTLE-PLAN ORDER TREE (engineer 73, 24 Nov 2026).
 *
 * Every follow-up battle-plan command names an order by its id inside an orders group (an army or an army group), so
 * the module needs the group's order tree before it can post one. The facts below are read from the game's own code
 * on the running build (never assumed), each from the function that uses it:
 *   - COrderInstance: its RTTI vtable, and its constructor (the one the "create order of kind" helper calls), which
 *     stores the kind (+0x30), the owning group (+0x38), the id (+0x214) and clears the executing byte (+0x218).
 *   - The group's three order lists (+0x98 the plan's top orders, +0xb0 and +0xc8 the fallback and area-defence
 *     orders) and the "find order by id" function itself: the lookup every order command's "can do" check calls
 *     (COrderSetPathCommand's check: lookup(group, id) != NULL). The module calls the SAME function (a pure read).
 *   - The subtree search that lookup calls: the order id field and the two child lists (+0x1c8: a front's arrows, an
 *     arrow's target line; +0x2a0: the target-line copy).
 *   - The province list (+0x70/+0x7c) and the assigned divisions (+0x1e0/+0x1ec): from the game's path setter (the
 *     function COrderSetPathCommand's execution jumps to).
 *   - The spearhead ("blitz") and withdraw bytes (+0x118/+0x119) and the drawing group (+0x348): from
 *     COrderNewFrontCommand's execution, which writes them into each order it creates; the lock handle (+0x228)
 *     from COrderSetPathCommand's check (a non-empty handle there and the game refuses a new path).
 * The reader is pure: it takes a fault-free read callback (the module's mach_vm_read wrapper; a recorded image in
 * the offline test) and prints the tree in the shape of the external runtime's battle-plan read
 * (native/src/launch/macos/armies/orders.rs), so tools and the executor see one model. */
#ifndef HOI4_INPROC_ORDERS_H
#define HOI4_INPROC_ORDERS_H
#include "scan.h"
#include <stddef.h>
#include <stdint.h>

typedef struct {
    int ok;
    uint64_t vt_order;            /* COrderInstance primary vtable (unslid) */
    uint64_t lookup_fn;           /* COrder* find(COrdersGroup *, int id) over the three lists (unslid) */
    uint64_t subtree_fn;          /* COrder* find_in(COrder *, int id) (unslid) */
    uint64_t path_fn;             /* the path setter (unslid; evidence only) */
    uint32_t kind_off, owner_off, id_off, exec_off;
    uint32_t list_off[3], list_cnt[3];          /* group: top, fallback, area defence */
    uint32_t child_off[2], child_cnt[2];        /* order: arrows/target, target copy */
    uint32_t provs_off, provs_cnt, divs_off, divs_cnt;
    uint32_t blitz_off, withdraw_off, drawer_off, lock_off;
    char evidence[400];
} orders_facts;

int scan_orders_facts(const scan_image *im, orders_facts *f);
const char *orders_facts_json(const orders_facts *f, char *buf, size_t n);

/* a fault-free read: 1 if all n bytes at addr were read */
typedef int (*orders_rd)(uint64_t addr, void *buf, size_t n);

/* The order tree of `group` as JSON members (no braces): "group":"0x..","lists":[{"list":"top|fallback|area",
 * "orders":[node,...]}...],"truncated":false. node = {"order":"0x..","kind":k,"id":i,"executing":b,"blitz":b,
 * "withdraw":b,"locked":b,"drawer":"0x..","provinces":[...],"divisions":n,"division_ptrs":["0x..",...],
 * "children":[node...],"target_copies":[node...]}. vt_order_slid = the vtable as loaded. Returns the number of orders
 * read, or -1 when an object is not what the facts say (a wrong vtable: nothing is guessed). */
int orders_tree_json(const orders_facts *f, orders_rd rd, uint64_t vt_order_slid, uint64_t group, char *out, size_t n);

/* An order of `group` by id, read the way the game's lookup walks (lists, then each subtree; depth <= 4).
 * Returns the order address or 0. The module calls the game's own lookup instead; this one serves the offline test
 * and cross-checks the facts (both must agree). */
uint64_t orders_find(const orders_facts *f, orders_rd rd, uint64_t vt_order_slid, uint64_t group, int32_t id);

/* engineer 74 (24 Nov): the FRONTS a plan root is bound to. A plan root (COrderNewRootCommand) is not a free province
 * list: the game binds it to a section of a CFront (the border between the group's country and an enemy, as the map's
 * front tool highlights it), stores that front's handle and the section id in the root (+0x228/+0x230, read from
 * NewRoot's own execution), and computes the root's provinces from the section and a fixed-point range [from, to] of
 * 0..100000 (+0x238/+0x240, read from the range setter EditRoot's execution calls). Every fact below is read from the
 * code that uses it:
 *   - a group's parent (+0x40, the COrdersGroup constructor's third argument): an army group, or the CTheatre;
 *   - the theatre's fronts list (+0x50, count +0x5c: the theatre's save loader appends each new CFront there);
 *   - a front's sections (+0x58, count +0x64) and a section's id (+8): the section lookup the root's path update calls
 *     with the root's section id;
 *   - a section's province pointers (+0x30, count +0x3c) and a province's id (+0x9c): the section-to-provinces copy the
 *     root's path update uses (count from its range-to-index arithmetic).
 * The CTheatre, CFront and CFrontSection objects are checked by their exact RTTI vtables at every step. */
typedef struct {
    int ok;
    uint64_t vt_theatre, vt_front, vt_section, vt_group;     /* unslid primary vtables */
    uint32_t group_parent_off, group_tag_off;                 /* COrdersGroup: parent (+0x40), owner tag (+0x3c) */
    uint32_t theatre_fronts_off, theatre_fronts_cnt;
    uint32_t front_sections_off, front_sections_cnt, section_id_off;
    uint32_t section_provs_off, section_provs_cnt, province_id_off;
    uint32_t root_front_off, root_section_off, root_range_off; /* in COrderInstance */
    uint64_t section_lookup_fn;
    char evidence[360];
} fronts_facts;

int scan_fronts_facts(const scan_image *im, fronts_facts *f);
const char *fronts_facts_json(const fronts_facts *f, char *buf, size_t n);
/* The group's theatre: the group's parent chain (at most 3 steps through COrdersGroup parents) to an object with the
 * CTheatre vtable. Returns 0 when none. */
uint64_t fronts_theatre_of(const fronts_facts *f, orders_rd rd, uint64_t vt_group_slid, uint64_t vt_theatre_slid, uint64_t group);
/* The theatre's fronts as JSON members (no braces): "theatre":"0x..","fronts":[{"front":"0x..","sections":[{"id":i,
 * "provinces":[...]}...]}...],"truncated":b. Returns the number of fronts, -1 when an object is not what the facts say. */
int fronts_json(const fronts_facts *f, orders_rd rd, uint64_t vt_front_slid, uint64_t vt_section_slid, uint64_t theatre,
                char *out, size_t n);
/* 1 when `front` is one of the theatre's fronts and has a section with this id (provinces > 0) */
int fronts_has_section(const fronts_facts *f, orders_rd rd, uint64_t vt_front_slid, uint64_t vt_section_slid, uint64_t theatre,
                       uint64_t front, int32_t section);

/* The divisions the UI would hand a NEW front drawn under `parent`: the parent's divisions that no child order of
 * the parent holds yet (the front tool's own rule, read from its code). Writes up to max pointers; returns the count
 * or -1 on a read fault. */
int orders_free_divisions(const orders_facts *f, orders_rd rd, uint64_t parent, uint64_t *out, int max);
/* engineer 74: the id of the order of `group` holding division `div` (0 none, -1 read fault), and the top-list order
 * bound to (front handle, section) (its id, 0 none): the read-backs of assign/unassign and new_root */
int32_t orders_holder(const orders_facts *f, orders_rd rd, uint64_t vt, uint64_t group, uint64_t div);
int32_t orders_root_on(const orders_facts *of, const fronts_facts *ff, orders_rd rd, uint64_t vt, uint64_t group, uint64_t front_handle,
                       int32_t section);
#endif
