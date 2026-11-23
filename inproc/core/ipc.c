/* HOI4 Live Tools in-process module: the local channel and the posting policy. See ipc.h. POSIX (macOS, Linux). */
#ifdef __linux__
#define _GNU_SOURCE /* SO_PEERCRED, struct ucred */
#endif
#include "ipc.h"
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <time.h>
#include <unistd.h>

const char *const IPC_ALLOWED[] = {"set_game_speed", "production", "air"};
const int IPC_ALLOWED_COUNT = 3;
const int IPC_LIVE_PASSED[] = {1 /* S1, 23 Nov */, 1 /* S2 production, 24 Nov */, 1 /* S2 air, 24 Nov */};

int ipc_family_of(const char *op) {
    const char *dot = op ? strchr(op, '.') : NULL;
    if (!dot) return -1;
    for (int i = 1; i < IPC_ALLOWED_COUNT; i++)
        if ((size_t)(dot - op) == strlen(IPC_ALLOWED[i]) && !strncmp(op, IPC_ALLOWED[i], (size_t)(dot - op))) return i;
    return -1;
}

static long long mono_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return (long long)t.tv_sec * 1000 + t.tv_nsec / 1000000;
}

int ipc_optin_valid(const char *list, char *why, size_t n) {
    if (!list || !*list) { snprintf(why, n, "no opt-in: HOI4_INPROC_POST not given"); return 0; }
    char buf[256]; snprintf(buf, sizeof buf, "%s", list);
    int any = 0;
    for (char *save = NULL, *tok = strtok_r(buf, ",", &save); tok; tok = strtok_r(NULL, ",", &save)) {
        int ok = 0;
        for (int i = 0; i < IPC_ALLOWED_COUNT; i++) if (!strcmp(tok, IPC_ALLOWED[i])) ok = 1;
        if (!ok) { snprintf(why, n, "opt-in names %.60s, which is not on the allow-list", tok); return 0; }
        any = 1;
    }
    if (!any) snprintf(why, n, "empty opt-in");
    return any;
}

int ipc_optin_has(const char *list, const char *name) {
    if (!list) return 0;
    char buf[256]; snprintf(buf, sizeof buf, "%s", list);
    for (char *save = NULL, *tok = strtok_r(buf, ",", &save); tok; tok = strtok_r(NULL, ",", &save))
        if (!strcmp(tok, name)) return 1;
    return 0;
}

/* A strict parser for one flat JSON object of at most `max` members whose values are short strings without escapes
 * or integers. Anything else (nesting, escapes, floats, duplicates, long values) is refused. */
int ipc_parse(const char *s, ipc_member *m, int max) {
    int n = 0;
    while (*s == ' ') s++;
    if (*s++ != '{') return -1;
    for (;;) {
        while (*s == ' ') s++;
        if (*s == '}' && n == 0) { s++; break; }
        if (*s++ != '"' || n >= max) return -1;
        ipc_member *e = &m[n]; memset(e, 0, sizeof *e);
        size_t k = 0;
        while (*s && *s != '"') { if (*s == '\\' || k + 1 >= sizeof e->key) return -1; e->key[k++] = *s++; }
        if (*s++ != '"') return -1;
        for (int j = 0; j < n; j++) if (!strcmp(m[j].key, e->key)) return -1;
        while (*s == ' ') s++;
        if (*s++ != ':') return -1;
        while (*s == ' ') s++;
        if (*s == '"') {
            s++; k = 0;
            while (*s && *s != '"') {
                if (*s == '\\' || (unsigned char)*s < 0x20 || k + 1 >= sizeof e->str) return -1;
                e->str[k++] = *s++;
            }
            if (*s++ != '"') return -1;
        } else {
            int neg = 0; int64_t v = 0; int digits = 0;
            if (*s == '-') { neg = 1; s++; }
            while (*s >= '0' && *s <= '9') { if (++digits > 12) return -1; v = v * 10 + (*s++ - '0'); }
            if (!digits) return -1;
            e->num = neg ? -v : v; e->is_num = 1;
        }
        n++;
        while (*s == ' ') s++;
        if (*s == ',') { s++; continue; }
        if (*s == '}') { s++; break; }
        return -1;
    }
    while (*s == ' ' || *s == '\r' || *s == '\n') s++;
    return *s ? -1 : n;
}

static const ipc_member *get(const ipc_member *m, int n, const char *key) {
    for (int i = 0; i < n; i++) if (!strcmp(m[i].key, key)) return &m[i];
    return NULL;
}

static int send_all(int fd, const char *s, size_t n) {
    while (n) {
#ifdef MSG_NOSIGNAL
        ssize_t w = send(fd, s, n, MSG_NOSIGNAL);
#else
        ssize_t w = send(fd, s, n, 0);
#endif
        if (w <= 0) { if (w < 0 && errno == EINTR) continue; return 0; }
        s += w; n -= (size_t)w;
    }
    return 1;
}

static int peer_is_us(int fd) {
#ifdef __linux__
    struct ucred c; socklen_t l = sizeof c;
    return getsockopt(fd, SOL_SOCKET, SO_PEERCRED, &c, &l) == 0 && c.uid == getuid();
#else
    uid_t u; gid_t g;
    return getpeereid(fd, &u, &g) == 0 && u == getuid();
#endif
}

static int make_token(const char *sock, char *tok, size_t tn) {
    unsigned char r[16];
    int fd = open("/dev/urandom", O_RDONLY | O_CLOEXEC);
    if (fd < 0 || read(fd, r, sizeof r) != (ssize_t)sizeof r) { if (fd >= 0) close(fd); return 0; }
    close(fd);
    for (int i = 0; i < 16 && 2 * i + 2 < (int)tn; i++) snprintf(tok + 2 * i, 3, "%02x", r[i]);
    char p[1100]; struct stat st;
    snprintf(p, sizeof p, "%s.token", sock);
    if (lstat(p, &st) == 0) {
        if (!S_ISREG(st.st_mode) || st.st_uid != getuid()) return 0;
        unlink(p);
    }
    fd = open(p, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600);
    if (fd < 0) return 0;
    int ok = write(fd, tok, 32) == 32;
    close(fd);
    return ok;
}

static int consttime_eq(const char *a, const char *b, size_t n) {
    unsigned char d = 0;
    if (strlen(a) != n || strlen(b) != n) return 0;
    for (size_t i = 0; i < n; i++) d |= (unsigned char)(a[i] ^ b[i]);
    return d == 0;
}

typedef struct {
    long long last_post, window[64];
    int nwin, in_flight;
    long long last2, win2[128]; int nwin2, cmds2[128];
} rate_state;

/* stage 2: at most p->stage2_jobs_per_minute requests and p->stage2_cmds_per_minute commands a minute, 50 ms apart */
static int rate2_ok(rate_state *r, const ipc_policy *p, int ncmds, char *why, size_t n) {
    long long now = mono_ms();
    int k = 0, cmds = 0;
    for (int i = 0; i < r->nwin2; i++)
        if (now - r->win2[i] < 60000) { r->win2[k] = r->win2[i]; r->cmds2[k] = r->cmds2[i]; cmds += r->cmds2[k]; k++; }
    r->nwin2 = k;
    if (r->in_flight) { snprintf(why, n, "a post is in flight"); return 0; }
    if (r->last2 && now - r->last2 < 50) { snprintf(why, n, "rate limit: %lld ms since the last request (minimum 50)", now - r->last2); return 0; }
    if (r->nwin2 >= p->stage2_jobs_per_minute || r->nwin2 >= 128) { snprintf(why, n, "rate limit: %d requests in the last minute", r->nwin2); return 0; }
    if (cmds + ncmds > p->stage2_cmds_per_minute) { snprintf(why, n, "rate limit: %d commands in the last minute", cmds); return 0; }
    return 1;
}

static int rate_ok(rate_state *r, const ipc_policy *p, char *why, size_t n) {
    long long now = mono_ms();
    int k = 0;
    for (int i = 0; i < r->nwin; i++) if (now - r->window[i] < 60000) r->window[k++] = r->window[i];
    r->nwin = k;
    if (r->in_flight) { snprintf(why, n, "a post is in flight"); return 0; }
    if (r->last_post && now - r->last_post < p->min_interval_ms) {
        snprintf(why, n, "rate limit: %lld ms since the last post (minimum %ld)", now - r->last_post, p->min_interval_ms);
        return 0;
    }
    if (r->nwin >= p->max_per_minute || r->nwin >= 64) { snprintf(why, n, "rate limit: %d posts in the last minute", r->nwin); return 0; }
    return 1;
}

static void handle(int fd, const ipc_backend *b, ipc_policy *p, rate_state *rs, const char *token) {
    char line[1100]; size_t at = 0; int authed = 0;
    static char frag[16384], out[17000];
    for (;;) {
        if (b->killed()) return;
        struct pollfd pf = {fd, POLLIN, 0};
        int pr = poll(&pf, 1, 1000);
        if (pr < 0 && errno == EINTR) continue;
        if (pr < 0) return;
        if (pr == 0) continue;
        ssize_t r = recv(fd, line + at, sizeof line - 1 - at, 0);
        if (r <= 0) return;
        at += (size_t)r; line[at] = 0;
        char *nl;
        while ((nl = strchr(line, '\n'))) {
            *nl = 0;
            ipc_member m[8]; int nm = ipc_parse(line, m, 8);
            const ipc_member *v = nm > 0 ? get(m, nm, "v") : NULL, *op = nm > 0 ? get(m, nm, "op") : NULL;
            const ipc_member *idm = nm > 0 ? get(m, nm, "id") : NULL;
            long id = idm && idm->is_num ? (long)idm->num : 0;
            int ok = 0; const char *err = NULL; char why[220] = ""; frag[0] = 0;
            if (nm <= 0 || !v || !v->is_num || !op || op->is_num) err = "malformed request";
            else if (v->num != IPC_PROTOCOL) err = "unsupported protocol version";
            else if (!strcmp(op->str, "hello")) {
                const ipc_member *t = get(m, nm, "token");
                if (!t || t->is_num || !consttime_eq(t->str, token, 32)) { err = "bad token"; authed = -1; }
                else {
                    authed = 1;
                    char h[6000] = "";
                    b->hello(h, sizeof h);
                    char allow[200] = ""; size_t aa = 0;
                    for (int i = 0; i < IPC_ALLOWED_COUNT; i++)
                        aa += (size_t)snprintf(allow + aa, sizeof allow - aa, "%s\"%s\"", i ? "," : "", IPC_ALLOWED[i]);
                    char fams[900] = ""; size_t fa = 0;
                    for (int i = 1; i < IPC_ALLOWED_COUNT && i < IPC_MAX_FAMILIES; i++)
                        fa += (size_t)snprintf(fams + fa, sizeof fams - fa, "%s\"%s\":{\"on\":%s,\"live_passed\":%s,\"reason\":\"%s\"}",
                                               i > 1 ? "," : "", IPC_ALLOWED[i], p->fam_on[i] ? "true" : "false",
                                               IPC_LIVE_PASSED[i] ? "true" : "false", p->fam_reason[i]);
                    snprintf(frag, sizeof frag,
                             "\"protocol\":%d,%s,\"post\":{\"enabled\":%s,\"reason\":\"%s\",\"allow\":[%s],"
                             "\"min_interval_ms\":%ld,\"max_per_minute\":%d},\"families\":{%s},"
                             "\"stage2_limits\":{\"max_objects\":32,\"requests_per_minute\":%d,\"commands_per_minute\":%d}",
                             IPC_PROTOCOL, h, p->post_enabled ? "true" : "false", p->post_off_reason, allow,
                             p->min_interval_ms, p->max_per_minute, fams, p->stage2_jobs_per_minute, p->stage2_cmds_per_minute);
                    ok = 1;
                }
            } else if (authed != 1) err = "hello first";
            else if (!strcmp(op->str, "read")) ok = b->read(frag, sizeof frag);
            else if (!strcmp(op->str, "probe")) ok = b->probe(frag, sizeof frag);
            else if (!strcmp(op->str, "post")) {
                const ipc_member *c = get(m, nm, "cmd"), *sp = get(m, nm, "speed");
                int allowed = 0, fam = c && !c->is_num ? ipc_family_of(c->str) : -1;
                for (int i = 0; c && !c->is_num && i < 1; i++) if (!strcmp(c->str, IPC_ALLOWED[i])) allowed = 1;
                if (fam > 0) { /* STAGE 2: a family command */
                    const ipc_member *ob = get(m, nm, "objs"), *va = get(m, nm, "vals"), *o2 = get(m, nm, "obj2"), *fl = get(m, nm, "flag");
                    int nobj = 0;
                    if (ob && !ob->is_num && ob->str[0]) { nobj = 1; for (const char *q = ob->str; *q; q++) if (*q == ',') nobj++; }
                    if (!p->post_enabled) { snprintf(why, sizeof why, "posting off: %s", p->post_off_reason); err = why; }
                    else if (fam >= IPC_MAX_FAMILIES || !p->fam_on[fam]) { snprintf(why, sizeof why, "family %s off: %s", IPC_ALLOWED[fam], fam < IPC_MAX_FAMILIES ? p->fam_reason[fam] : "?"); err = why; }
                    else if (!ob || ob->is_num || nobj < 1 || nobj > 32) err = "objs: 1 to 32 comma-separated object addresses";
                    else if ((va && va->is_num) || (o2 && o2->is_num) || (fl && !fl->is_num)) err = "member types";
                    else if (!rate2_ok(rs, p, nobj, why, sizeof why)) err = why;
                    else {
                        int unexplained = 0;
                        rs->in_flight = 1;
                        long long t0 = mono_ms();
                        ok = b->post2(c->str, ob->str, va ? va->str : "", o2 ? o2->str : "", fl ? fl->num : 0, id, frag, sizeof frag, &unexplained);
                        rs->in_flight = 0;
                        rs->last2 = t0;
                        if (rs->nwin2 < 128) { rs->win2[rs->nwin2] = t0; rs->cmds2[rs->nwin2] = nobj; rs->nwin2++; }
                        if (unexplained) {
                            p->fam_on[fam] = 0;
                            snprintf(p->fam_reason[fam], sizeof p->fam_reason[fam], "switched off after an unexplained refusal");
                            b->log("\"event\":\"family_off\",\"reason\":\"unexplained refusal\"");
                        }
                    }
                } else if (!p->post_enabled) { snprintf(why, sizeof why, "posting off: %s", p->post_off_reason); err = why; }
                else if (!allowed) err = "command not on the allow-list";
                else if (!p->fam_on[0]) err = "set_game_speed not named in the launcher's opt-in";
                else if (nm != (idm ? 5 : 4)) err = "unexpected members for set_game_speed";
                else if (!sp || !sp->is_num || sp->num < 0 || sp->num >= p->speed_max) err = "speed out of the scanned range";
                else if (!rate_ok(rs, p, why, sizeof why)) err = why;
                else {
                    int unexplained = 0;
                    rs->in_flight = 1;
                    long long t0 = mono_ms();
                    ok = b->post_speed(sp->num, id, frag, sizeof frag, &unexplained);
                    rs->in_flight = 0;
                    rs->last_post = t0;
                    if (rs->nwin < 64) rs->window[rs->nwin++] = t0;
                    if (!ok && unexplained) {
                        p->post_enabled = 0;
                        snprintf(p->post_off_reason, sizeof p->post_off_reason, "switched off after an unexplained refusal");
                        b->log("\"event\":\"posting_off\",\"reason\":\"unexplained refusal\"");
                    }
                }
                if (err) {
                    char lg[400];
                    snprintf(lg, sizeof lg, "\"event\":\"post\",\"id\":%ld,\"cmd\":\"%s\",\"result\":\"refused_by_policy\",\"reason\":\"%s\"",
                             id, c && !c->is_num ? c->str : "?", err);
                    b->log(lg);
                }
            } else err = "unknown op";
            int w = snprintf(out, sizeof out, "{\"v\":%d,\"id\":%ld,\"op\":\"%s\",\"ok\":%s%s%s%s%s%s}\n", IPC_PROTOCOL, id,
                             op && !op->is_num ? op->str : "", ok && !err ? "true" : "false", frag[0] ? "," : "", frag,
                             err ? ",\"error\":\"" : "", err ? err : "", err ? "\"" : "");
            if (w <= 0 || w >= (int)sizeof out || !send_all(fd, out, (size_t)w)) return;
            if (authed == -1) return; /* a wrong token ends the connection */
            size_t rest = at - (size_t)(nl + 1 - line);
            memmove(line, nl + 1, rest + 1); at = rest;
        }
        if (at >= sizeof line - 1) return; /* a line longer than 1 KiB: drop the client */
    }
}

int ipc_serve(const char *sock, const ipc_backend *b, ipc_policy *p) {
    char lg[600];
    struct sockaddr_un a; memset(&a, 0, sizeof a); a.sun_family = AF_UNIX;
    if (!sock || strlen(sock) >= sizeof a.sun_path) { b->log("\"event\":\"ipc_off\",\"reason\":\"socket path missing or too long\""); return 0; }
    snprintf(a.sun_path, sizeof a.sun_path, "%s", sock);
    char dir[sizeof a.sun_path]; snprintf(dir, sizeof dir, "%s", sock);
    char *sl = strrchr(dir, '/');
    struct stat st;
    if (!sl) { b->log("\"event\":\"ipc_off\",\"reason\":\"socket path must be absolute\""); return 0; }
    *sl = 0;
    if (stat(dir, &st) || !S_ISDIR(st.st_mode) || st.st_uid != getuid() || (st.st_mode & 077)) {
        b->log("\"event\":\"ipc_off\",\"reason\":\"socket folder must be ours and mode 0700\"");
        return 0;
    }
    if (lstat(sock, &st) == 0) {
        if (!S_ISSOCK(st.st_mode) || st.st_uid != getuid()) { b->log("\"event\":\"ipc_off\",\"reason\":\"socket path taken\""); return 0; }
        unlink(sock);
    }
    char token[40] = "";
    if (!make_token(sock, token, sizeof token)) { b->log("\"event\":\"ipc_off\",\"reason\":\"no token file\""); return 0; }
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) { b->log("\"event\":\"ipc_off\",\"reason\":\"socket()\""); return 0; }
    fcntl(fd, F_SETFD, FD_CLOEXEC);
    if (bind(fd, (struct sockaddr *)&a, sizeof a) || chmod(sock, 0600) || listen(fd, 1)) {
        snprintf(lg, sizeof lg, "\"event\":\"ipc_off\",\"reason\":\"bind/listen: %s\"", strerror(errno));
        b->log(lg); close(fd); return 0;
    }
    snprintf(lg, sizeof lg, "\"event\":\"ipc_listening\",\"protocol\":%d,\"post_enabled\":%s,\"reason\":\"%s\"",
             IPC_PROTOCOL, p->post_enabled ? "true" : "false", p->post_off_reason);
    b->log(lg);
    rate_state rs; memset(&rs, 0, sizeof rs);
    while (!b->killed()) {
        struct pollfd pf = {fd, POLLIN, 0};
        int pr = poll(&pf, 1, 1000);
        if (pr <= 0) continue;
        int c = accept(fd, NULL, NULL);
        if (c < 0) continue;
        fcntl(c, F_SETFD, FD_CLOEXEC);
#ifdef SO_NOSIGPIPE
        int one = 1; setsockopt(c, SOL_SOCKET, SO_NOSIGPIPE, &one, sizeof one);
#endif
        if (!peer_is_us(c)) { b->log("\"event\":\"ipc_reject\",\"reason\":\"peer is another user\""); close(c); continue; }
        b->log("\"event\":\"ipc_client\"");
        handle(c, b, p, &rs, token);
        close(c);
    }
    close(fd); unlink(sock);
    b->log("\"event\":\"ipc_closed\"");
    return 1;
}
