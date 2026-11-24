/* HOI4 Live Tools in-process module: the local channel and the posting policy (stage 1).
 *
 * A Unix-domain stream socket at a path the launcher chooses (inside the run folder, mode 0700), created mode 0600,
 * every peer checked to be this user (getpeereid) and then to present the token the module writes beside the socket
 * (<sock>.token, 0600, fresh random per game process). One client at a time; newline-delimited JSON, protocol v1:
 *   {"v":1,"op":"hello","token":"..."}            -> capabilities, whether posting is enabled and why not, limits
 *   {"v":1,"op":"read"}                           -> date, speed, paused, player (the module's own reads)
 *   {"v":1,"op":"probe"}                          -> runs the main-thread checks without posting anything
 *   {"v":1,"op":"post","cmd":"set_game_speed","speed":N,"id":K}
 *                                                 -> posted or refused (reason), timings, the read-back
 * The policy lives here so it is the same on every OS: a hard allow-list (stage 1: exactly one command), an explicit
 * opt-in by name from the launcher, every field range-checked against the scanned layout, one post in flight, a rate
 * limit, posting switched off for the rest of the process after any refusal the module cannot explain. */
#ifndef HOI4_INPROC_IPC_H
#define HOI4_INPROC_IPC_H
#include <stddef.h>
#include <stdint.h>

#define IPC_PROTOCOL 1

typedef struct {
    /* each writes a JSON fragment (members without the braces) into out and returns 1, or 0 on failure */
    int (*hello)(char *out, size_t n);
    int (*read)(char *out, size_t n);
    int (*probe)(char *out, size_t n);
    /* posts set_game_speed(value) after the policy passed; returns 1 posted and read back, 0 refused/failed.
     * *unexplained is set when the failure is one the module cannot explain (posting is then switched off). */
    int (*post_speed)(int64_t value, long id, char *out, size_t n, int *unexplained);
    /* STAGE 2: posts one family command for each listed object (objs, comma-separated hex addresses of own game
     * objects the client read; vals, per-object integers; obj2, a second object (air base, strategic region); flag).
     * Returns 1 when every object was handled (posted and, where the module can, read back), 0 otherwise. */
    int (*post2)(const char *op, const char *objs, const char *vals, const char *obj2, const char *provs, const char *divs,
                 int64_t flag, long id, char *out, size_t n, int *unexplained);
    /* engineer 73: the battle-plan order tree of one own orders group (army or army group; hex address): a pure read,
     * served whenever the order facts resolved (no opt-in needed: it reads only the player's own orders) */
    int (*orders)(const char *group, char *out, size_t n);
    /* engineer 74: the fronts of an own group's theatre (the CFront sections a plan root binds to): a pure read of the
     * player's own theatre, served whenever the front facts resolved */
    int (*fronts)(const char *group, char *out, size_t n);
    int (*killed)(void);
    void (*log)(const char *json_members);
} ipc_backend;

#define IPC_MAX_FAMILIES 16
typedef struct {
    int post_enabled;           /* opt-in given AND every capability resolved */
    /* stage 2: per family (IPC_ALLOWED[1..]), opted in by the launcher AND every command type of it resolved */
    int fam_on[IPC_MAX_FAMILIES];
    char fam_reason[IPC_MAX_FAMILIES][120];
    int stage2_jobs_per_minute, stage2_cmds_per_minute;
    char post_off_reason[200];
    int64_t speed_max;          /* exclusive upper bound from the scan (5) */
    long min_interval_ms;       /* between two posts */
    int max_per_minute;
} ipc_policy;

/* The compiled-in allow-list: stage 1's one command, then the stage-2 FAMILIES. An entry posts only when the
 * launcher's opt-in names it and every scan behind it resolved. IPC_LIVE_PASSED says which entries passed their live
 * session; the backend switch's "auto" uses only those (a family not yet proven live runs only when named explicitly). */
extern const char *const IPC_ALLOWED[];
extern const int IPC_ALLOWED_COUNT;
extern const int IPC_LIVE_PASSED[];
/* the family of a stage-2 op ("air.transfer" -> index of "air"), or -1 */
int ipc_family_of(const char *op);
/* 1 if every name in the comma-separated opt-in list is on the allow-list (and the list is not empty). */
int ipc_optin_valid(const char *list, char *why, size_t n);
int ipc_optin_has(const char *list, const char *name);

/* Creates the socket and token, then serves until the kill switch; returns 0 on a setup failure (logged). */
int ipc_serve(const char *sock_path, const ipc_backend *b, ipc_policy *p);

/* Exposed for the offline test: parse one request line into at most 8 members (strings or integers). */
typedef struct { char key[24]; char str[8192]; int64_t num; int is_num; } ipc_member; /* 8 KiB: a front's province list */
int ipc_parse(const char *line, ipc_member *m, int max);
#endif
