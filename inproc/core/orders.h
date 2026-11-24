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

/* The divisions the UI would hand a NEW front drawn under `parent`: the parent's divisions that no child order of
 * the parent holds yet (the front tool's own rule, read from its code). Writes up to max pointers; returns the count
 * or -1 on a read fault. */
int orders_free_divisions(const orders_facts *f, orders_rd rd, uint64_t parent, uint64_t *out, int max);
#endif
