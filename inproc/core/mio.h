/* HOI4 Live Tools in-process module: the MILITARY-INDUSTRIAL ORGANISATIONS read (engineer 84, 24 Nov 2026).
 *
 * The player's own organisations as the organisation views show them: each organisation's key, funds, size, unspent
 * trait points, attached policy, unlock queue and its trait tree in the game's own order with each trait's state
 * (unlocked / queued / unlockable now / locked), and the country's policies with, per organisation, the ones the policy
 * window's own gate allows. Every offset is a fact the scan read from the code that uses it (mio_facts in cmd2.h).
 *
 * The reader is pure apart from three callbacks the module provides on the main thread: a fault-free read, the game's
 * token-to-name function (returns the address of a libc++ std::string) and the game's own checks (can_unlock; the
 * policy item's gate). The offline test drives it with a synthetic image. */
#ifndef HOI4_INPROC_MIO_H
#define HOI4_INPROC_MIO_H
#include "cmd2.h"
#include <stddef.h>
#include <stdint.h>

typedef int (*mio_rd)(uint64_t addr, void *buf, size_t n);
typedef struct {
    mio_rd rd;
    uint64_t (*name)(void *ctx, int32_t token);                       /* address of the token's std::string, 0 = none */
    int (*can_unlock)(void *ctx, uint64_t org, uint64_t trait);       /* 1 = the game's can_unlock accepts it now */
    int (*policy_ok)(void *ctx, uint64_t org, uint64_t handle, int32_t policy); /* 1 = the policy item's gate allows the click */
    void *ctx;
    uint64_t vt_org; /* slid */
} mio_env;

/* the organisation list of the country's manager (mgr: the slid pointer +mgr_off of the player's country); writes the
 * members (no braces) and returns the number of organisations, or -1 when something is not what the facts say */
int mio_json(const mio_facts *f, const mio_env *e, uint64_t mgr, int32_t player_tag, char *out, size_t cap);

/* helpers the module's posts share */
int mio_in_list(const mio_facts *f, mio_rd rd, uint64_t mgr, uint64_t org);              /* 1 = an organisation of mgr's list */
uint64_t mio_trait(const mio_facts *f, mio_rd rd, uint64_t org, int32_t token);           /* the tree's template, 0 = none */
uint64_t mio_policy(const mio_facts *f, mio_rd rd, uint64_t mgr, int32_t token);          /* the country's CPolicy, 0 = none */
int mio_queued(const mio_facts *f, mio_rd rd, uint64_t org, int32_t token);               /* 1 queued, 0 not, -1 unreadable */
int mio_unlocked(const mio_facts *f, mio_rd rd, uint64_t org, int32_t token);             /* 1 unlocked, 0 not, -1 unreadable */
int mio_str(mio_rd rd, uint64_t s, char *buf, size_t n);                                  /* a libc++ std::string, JSON-safe */
#endif
