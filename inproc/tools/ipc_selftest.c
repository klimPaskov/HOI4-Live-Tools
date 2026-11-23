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
int main(int argc, char **argv) {
    if (argc < 2) return 2;
    g_sock = argv[1];
    char why[200];
    printf("{\"optin_ok\":%d,\"optin_bad\":%d,\"optin_empty\":%d}\n", ipc_optin_valid("set_game_speed", why, sizeof why),
           ipc_optin_valid("set_game_speed,set_research", why, sizeof why), ipc_optin_valid("", why, sizeof why));
    ipc_member m[8];
    printf("{\"parse_ok\":%d,\"parse_nested\":%d,\"parse_escape\":%d,\"parse_dup\":%d,\"parse_float\":%d}\n",
           ipc_parse("{\"v\":1,\"op\":\"post\",\"speed\":-2}", m, 8), ipc_parse("{\"v\":{\"a\":1}}", m, 8),
           ipc_parse("{\"op\":\"a\\\"b\"}", m, 8), ipc_parse("{\"v\":1,\"v\":2}", m, 8), ipc_parse("{\"v\":1.5}", m, 8));
    fflush(stdout);
    ipc_policy p = {1, "on: selftest", 5, 200, 60};
    ipc_backend b = {hello, rd, probe, post, killed, lg};
    return ipc_serve(g_sock, &b, &p) ? 0 : 1;
}
