/* fake_host: a harmless x86_64 stand-in for the load test. Installed by test_load.sh at a path ending in
 * /Contents/MacOS/hoi4 so the module's constructor treats it as the game. It reports whether the module removed
 * DYLD_INSERT_LIBRARIES from the environment, then idles long enough for the module's worker to scan it (the
 * scan must fail gracefully: this image is not the game). */
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

int main(int argc, char **argv) {
    const char *e = getenv("DYLD_INSERT_LIBRARIES"), *p = getenv("HOI4_INPROC_POST"), *s = getenv("HOI4_INPROC_SOCK");
    printf("{\"dyld_insert_libraries_after_load\":%s%s%s,\"stage1_switches_after_load\":%s}\n", e ? "\"" : "",
           e ? e : "null", e ? "\"" : "", p || s ? "\"present\"" : "null");
    fflush(stdout);
    sleep(argc > 1 ? (unsigned)atoi(argv[1]) : 4);
    return 0;
}
