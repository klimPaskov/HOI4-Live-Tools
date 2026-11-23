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
    int (*killed)(void);
    void (*log)(const char *json_members);
} ipc_backend;

typedef struct {
    int post_enabled;           /* opt-in given AND every capability resolved */
    char post_off_reason[200];
    int64_t speed_max;          /* exclusive upper bound from the scan (5) */
    long min_interval_ms;       /* between two posts */
    int max_per_minute;
} ipc_policy;

/* The compiled-in allow-list. Stage 1: exactly one entry. */
extern const char *const IPC_ALLOWED[];
extern const int IPC_ALLOWED_COUNT;
/* 1 if every name in the comma-separated opt-in list is on the allow-list (and the list is not empty). */
int ipc_optin_valid(const char *list, char *why, size_t n);

/* Creates the socket and token, then serves until the kill switch; returns 0 on a setup failure (logged). */
int ipc_serve(const char *sock_path, const ipc_backend *b, ipc_policy *p);

/* Exposed for the offline test: parse one request line into at most 8 members (strings or integers). */
typedef struct { char key[24]; char str[72]; int64_t num; int is_num; } ipc_member;
int ipc_parse(const char *line, ipc_member *m, int max);
#endif
