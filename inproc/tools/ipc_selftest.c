/* ipc_selftest: the channel and the posting policy (core/ipc.c) with a mock game, for the offline test. Nothing here
 * touches a game. usage: ipc_selftest SOCKET_PATH  (serves until SOCKET_PATH.off exists). The mock "posts" by
 * recording the value; speed 3 simulates a refusal the module cannot explain (posting must then switch off). */
#include "../core/ipc.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

static const char *g_sock;
static int g_speed = 0;
static int killed(void) { char p[1100]; struct stat st; snprintf(p, sizeof p, "%s.off", g_sock); return stat(p, &st) == 0; }
static void lg(const char *m) { fprintf(stderr, "{%s}\n", m); }
static int hello(char *o, size_t n) { snprintf(o, n, "\"module\":\"selftest\",\"capabilities\":{\"mock\":true}"); return 1; }
static int rd(char *o, size_t n) { snprintf(o, n, "\"in_game\":true,\"speed\":%d", g_speed); return 1; }
static int probe(char *o, size_t n) {
    snprintf(o, n, "\"main_thread\":true,\"queue_type\":\"8CSession\",\"server_type\":\"12CDummyServer\",\"would_post\":true");
    return 1;
}
static int post(int64_t v, long id, char *o, size_t n, int *unexplained) {
    if (v == 3) { *unexplained = 1; snprintf(o, n, "\"result\":\"dropped_by_post\",\"id\":%ld", id); return 0; }
    g_speed = (int)v;
    char p[1100]; snprintf(p, sizeof p, "%s.speed", g_sock); /* what the mock "external runtime" reads */
    FILE *f = fopen(p, "w"); if (f) { fprintf(f, "{\"speed\":%d}\n", g_speed); fclose(f); }
    snprintf(o, n, "\"result\":\"executed\",\"speed_after\":%d,\"post_us\":3,\"main_wait_ms\":8.0,\"readback_ms\":12.0,\"total_ms\":20.0,\"requeues\":0", g_speed);
    return 1;
}
/* stage 2 mock: objects "0x1".."0x20" are own and valid; 0xbad is foreign (refused before posting); 0xdead makes the
 * game's own check say no; 0x666 simulates an unexplained refusal (the family must switch off) */
static int post2(const char *op, const char *objs, const char *vals, const char *obj2, const char *provs, const char *divs, int64_t flag, long id,
                 char *o, size_t n, int *unexplained) {
    (void)vals; (void)obj2; (void)flag; (void)provs; (void)divs;
    if (strstr(objs, "0x666")) { *unexplained = 1; snprintf(o, n, "\"op\":\"%s\",\"result\":\"dropped_by_post\",\"id\":%ld", op, id); return 0; }
    int posted = 0, refused = 0;
    char b[700]; snprintf(b, sizeof b, "%s", objs);
    for (char *s = NULL, *t = strtok_r(b, ",", &s); t; t = strtok_r(NULL, ",", &s)) {
        if (!strcmp(t, "0xbad") || !strcmp(t, "0xdead")) refused++; else posted++;
    }
    snprintf(o, n, "\"op\":\"%s\",\"posted\":%d,\"refused\":%d", op, posted, refused);
    return refused == 0;
}
/* engineer 73: the order-tree read (mock: one front with one arrow) */
static int fronts(const char *group, char *o, size_t n) {
    snprintf(o, n, "\"theatre\":\"0x9\",\"fronts\":[{\"front\":\"0x10\",\"sections\":[{\"id\":1,\"provinces\":[11,12]}]}],\"truncated\":false");
    (void)group; return 1;
}
static int mios(char *o, size_t n) {
    snprintf(o, n, "\"policies\":[],\"empty_token\":19479,\"organisations\":[{\"org\":\"0x10\",\"key\":\"ita_ansaldo\",\"traits\":[[7,\"t\",\"a\"]]}],\"organisations_read\":1,\"truncated\":false");
    return 1;
}
static int orders(const char *group, char *o, size_t n) {
    snprintf(o, n, "\"group\":\"%s\",\"lists\":[{\"list\":\"top\",\"orders\":[]}],\"orders_read\":0,\"truncated\":false", group);
    return 1;
}
int main(int argc, char **argv) {
    if (argc < 2) return 2;
    g_sock = argv[1];
    char why[200];
    printf("{\"optin_ok\":%d,\"optin_bad\":%d,\"optin_empty\":%d,\"optin_families\":%d,\"family_of\":[%d,%d,%d,%d]}\n",
           ipc_optin_valid("set_game_speed", why, sizeof why),
           ipc_optin_valid("set_game_speed,set_research", why, sizeof why), ipc_optin_valid("", why, sizeof why),
           ipc_optin_valid("set_game_speed,production,air", why, sizeof why), ipc_family_of("production.add_factories"),
           ipc_family_of("air.transfer"), ipc_family_of("console.move"), ipc_family_of("set_game_speed"));
    static ipc_member m[8];
    printf("{\"parse_ok\":%d,\"parse_nested\":%d,\"parse_escape\":%d,\"parse_dup\":%d,\"parse_float\":%d}\n",
           ipc_parse("{\"v\":1,\"op\":\"post\",\"speed\":-2}", m, 8), ipc_parse("{\"v\":{\"a\":1}}", m, 8),
           ipc_parse("{\"op\":\"a\\\"b\"}", m, 8), ipc_parse("{\"v\":1,\"v\":2}", m, 8), ipc_parse("{\"v\":1.5}", m, 8));
    fflush(stdout);
    ipc_policy p;
    memset(&p, 0, sizeof p);
    p.post_enabled = 1; snprintf(p.post_off_reason, sizeof p.post_off_reason, "on: selftest");
    p.speed_max = 5; p.min_interval_ms = 200; p.max_per_minute = 60;
    p.fam_on[0] = 1; p.fam_on[1] = !getenv("IPC_SELFTEST_SPEED_ONLY"); p.fam_on[2] = 0; /* production on, air off (not resolved in the mock) */
    snprintf(p.fam_reason[1], sizeof p.fam_reason[1], "on"); snprintf(p.fam_reason[2], sizeof p.fam_reason[2], "not opted in"); snprintf(p.fam_reason[3], sizeof p.fam_reason[3], "not opted in"); snprintf(p.fam_reason[4], sizeof p.fam_reason[4], "not opted in"); snprintf(p.fam_reason[5], sizeof p.fam_reason[5], "not opted in");
    p.stage2_jobs_per_minute = 600; p.stage2_cmds_per_minute = 2400;
    ipc_backend b = {hello, rd, probe, post, post2, orders, fronts, mios, killed, lg};
    return ipc_serve(g_sock, &b, &p) ? 0 : 1;
}
