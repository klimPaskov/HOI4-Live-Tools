/* HOI4 Live Tools in-process module, macOS loader, STAGE 0 (read-only).
 *
 * Loaded only when the project's own launcher starts the game with DYLD_INSERT_LIBRARIES set (opt-in, default
 * off; the game's files are never changed). What it does, and all it does:
 *   1. In the constructor: stays inert unless the host is the HOI4 executable and HOI4_INPROC_LOG names a log;
 *      removes DYLD_INSERT_LIBRARIES from the environment so programs the game starts do not load it; logs "loaded".
 *   2. On its own thread, after a delay: scans the game's image in memory with the same scanner the offline test
 *      runs on the executable file (core/scan.c), writes the capability table to the log and to a per-build cache.
 *   3. Waits for a loaded game: finds the game-state and idler roots by their RTTI dynamic type, then logs three
 *      reads (date, speed and pause, player tag) for comparison with the external runtime's hoi4_overview.
 * It never calls a game function, installs no hook, writes no game memory and posts no command. Every read of
 * game memory goes through vm_read_overwrite on its own task, which returns an error instead of faulting, so a
 * stale pointer cannot crash the game. Kill switch: a file named <log>.off makes it go inert at the next poll. */
#include "../core/caps.h"
#include <dlfcn.h>
#include <fcntl.h>
#include <mach-o/dyld.h>
#include <mach/mach.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#define MODULE_VERSION "stage0-1"
static int g_log = -1;
static char g_log_path[1024];
static long g_log_bytes;

static long long now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_REALTIME, &t);
    return (long long)t.tv_sec * 1000 + t.tv_nsec / 1000000;
}

static void logj(const char *fmt, ...) {
    if (g_log < 0 || g_log_bytes > (8L << 20)) return; /* bounded: at most 8 MiB per game process */
    char body[12288], line[12400];
    va_list ap; va_start(ap, fmt); vsnprintf(body, sizeof body, fmt, ap); va_end(ap);
    int n = snprintf(line, sizeof line, "{\"t\":%lld,%s}\n", now_ms(), body);
    if (n > 0) { ssize_t w = write(g_log, line, (size_t)(n < (int)sizeof line ? n : (int)sizeof line - 1)); if (w > 0) g_log_bytes += w; }
}

/* Fault-free read of this process's memory. */
static int sread(uint64_t addr, void *buf, size_t n) {
    vm_size_t got = 0;
    if (addr < 0x10000 || addr > 0x7fffffff0000ULL) return 0;
    kern_return_t kr = vm_read_overwrite(mach_task_self(), (vm_address_t)addr, (vm_size_t)n, (vm_address_t)buf, &got);
    return kr == KERN_SUCCESS && got == n;
}
static int sq(uint64_t a, uint64_t *v) { return sread(a, v, 8); }
static int plausible(uint64_t p) { return p >= 0x10000 && p <= 0x7fffffff0000ULL; }

static int killed(void) {
    char p[1100]; struct stat st;
    snprintf(p, sizeof p, "%s.off", g_log_path);
    return stat(p, &st) == 0;
}

/* libc++ std::string (x86_64): short form when bit 0 of the first byte is clear */
static int read_string(uint64_t a, char *out, size_t cap) {
    uint8_t h[24];
    if (!sread(a, h, 24)) return 0;
    size_t size; uint64_t ptr = 0;
    if (!(h[0] & 1)) { size = h[0] >> 1; if (size > 22 || size + 1 > cap) return 0; memcpy(out, h + 1, size); }
    else {
        uint64_t c, s; memcpy(&c, h, 8); memcpy(&s, h + 8, 8); memcpy(&ptr, h + 16, 8);
        if (s + 1 > cap || s > (c & ~1ULL) || !sread(ptr, out, (size_t)s)) return 0;
        size = (size_t)s;
    }
    out[size] = 0;
    return 1;
}

/* the external runtime's calendar (native/src/calendar.rs) */
static int date_text(int32_t raw, char *out, size_t n) {
    static const int months[12] = {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};
    long long hours = (long long)raw - 43800000LL;
    long long year = hours >= 0 ? hours / 8760 : -((-hours + 8759) / 8760);
    if (year < 1936 || year > 2000) return 0;
    long long rem = hours - year * 8760, day = rem / 24, hour = rem % 24 + 1;
    int month = 1;
    for (int i = 0; i < 12 && day >= months[i]; i++) { day -= months[i]; month++; }
    snprintf(out, n, "%04lld.%02d.%02lld.%02lld", year, month, day + 1, hour);
    return 1;
}

typedef struct {
    int ok;
    int32_t date, speed, tag;
    uint8_t paused;
    char date_s[24], tag_s[8];
} reads_t;

static void *worker(void *arg) {
    (void)arg;
    pthread_setname_np("hoi4-live-inproc");
    const char *d = getenv("HOI4_INPROC_DELAY_S");
    unsigned delay = d ? (unsigned)atoi(d) : 20;
    sleep(delay); /* let the game finish starting before any work */
    if (killed()) { logj("\"event\":\"inert\",\"reason\":\"kill switch\""); return NULL; }

    scan_image im; caps_table t;
    long long t0 = now_ms();
    int64_t slide = _dyld_get_image_vmaddr_slide(0);
    if (!scan_image_open_process(&im, _dyld_get_image_header(0), slide)) {
        logj("\"event\":\"inert\",\"reason\":\"main image is not an x86_64 Mach-O the scanner can read\"");
        return NULL;
    }
    int resolved = caps_resolve(&im, &t);
    long long t1 = now_ms();
    static char js[16384];
    logj("\"event\":\"capabilities\",\"resolved\":%d,\"of\":%d,\"scan_ms\":%lld,\"slide\":\"0x%llx\",\"table\":%s",
         resolved, CAP_COUNT, t1 - t0, (unsigned long long)slide, caps_json(&im, &t, js, sizeof js));
    { /* per-build cache, read by `doctor`; the module rescans every start (the scan is cheap), so a stale cache is harmless */
        char p[1200], dir[1024];
        const char *c = getenv("HOI4_INPROC_CACHE");
        snprintf(dir, sizeof dir, "%s", c ? c : g_log_path);
        if (!c) { char *s = strrchr(dir, '/'); if (s) *s = 0; }
        snprintf(p, sizeof p, "%s/inproc-caps-%s.json", dir, im.uuid);
        int fd = open(p, O_WRONLY | O_CREAT | O_TRUNC, 0600);
        if (fd >= 0) { ssize_t w = write(fd, js, strlen(js)); (void)w; close(fd); }
    }
    if (t.cap[CAP_RESEARCH_FACTORY].ok) { /* cross-check: the dynamic linker finds the same factory by name */
        void *s = dlsym(RTLD_DEFAULT, "_ZN14CCommandHelperI19CSetResearchCommandE18CommandRegistrator6CreateEv");
        logj("\"event\":\"dlsym_check\",\"dlsym\":\"%p\",\"scan\":\"0x%llx\",\"match\":%s", s,
             (unsigned long long)(t.cap[CAP_RESEARCH_FACTORY].value + (uint64_t)slide),
             (uint64_t)(uintptr_t)s == t.cap[CAP_RESEARCH_FACTORY].value + (uint64_t)slide ? "true" : "false");
    }
    if (!t.cap[CAP_GAMESTATE_VTABLE].ok || !t.cap[CAP_IDLER_VTABLE].ok) {
        logj("\"event\":\"inert\",\"reason\":\"roots unresolved: reads off\"");
        scan_image_free(&im);
        return NULL;
    }
    uint64_t *cand; unsigned *refs;
    size_t nc = scan_global_candidates(&im, 200, &cand, &refs);
    uint64_t vt_gs = t.cap[CAP_GAMESTATE_VTABLE].value + (uint64_t)slide;
    uint64_t vt_idler = t.cap[CAP_IDLER_VTABLE].value + (uint64_t)slide;
    uint64_t g_gs = 0, g_idler = 0;
    int loaded = 0, polls = 0;
    reads_t last; memset(&last, 0, sizeof last);
    long long last_log = 0;
    logj("\"event\":\"waiting\",\"global_candidates\":%zu", nc);
    for (;;) {
        sleep(2);
        polls++;
        if (killed()) { logj("\"event\":\"inert\",\"reason\":\"kill switch\""); break; }
        if (!g_gs || !g_idler) { /* roots by dynamic type: the global whose object's vtable is the RTTI-found one */
            for (size_t i = 0; i < nc; i++) {
                uint64_t p, vp;
                if (!sq(cand[i] + (uint64_t)slide, &p) || !plausible(p) || !sq(p, &vp)) continue;
                if (vp == vt_gs && !g_gs) g_gs = cand[i];
                if (vp == vt_idler && !g_idler) g_idler = cand[i];
            }
            if (g_gs && g_idler)
                logj("\"event\":\"roots\",\"gamestate_global\":\"0x%llx\",\"idler_global\":\"0x%llx\","
                     "\"idler_is_queue_root\":%s,\"polls\":%d",
                     (unsigned long long)g_gs, (unsigned long long)g_idler,
                     g_idler == t.cap[CAP_QUEUE_ROOT].value ? "true" : "false", polls);
            continue;
        }
        if (!t.fields) {
            if (!loaded) { logj("\"event\":\"reads_off\",\"reason\":\"no field table for this build\""); loaded = 1; }
            continue;
        }
        reads_t r; memset(&r, 0, sizeof r);
        uint64_t gs, idler, vp;
        const caps_fields *f = t.fields;
        if (sq(g_gs + (uint64_t)slide, &gs) && sq(g_idler + (uint64_t)slide, &idler) && sq(gs, &vp) && vp == vt_gs &&
            sq(idler, &vp) && vp == vt_idler && sread(gs + f->date, &r.date, 4) && sread(gs + f->speed, &r.speed, 4) &&
            sread(idler + f->paused, &r.paused, 1) && sread(gs + f->player_tag, &r.tag, 4) &&
            date_text(r.date, r.date_s, sizeof r.date_s) && r.speed >= 0 && r.speed <= 4 && r.paused <= 1 &&
            r.tag > 0 && r.tag < 4096) {
            uint64_t db, data; uint8_t hdr[16];
            if (sq(f->tag_db_global + (uint64_t)slide, &db) && sread(db, hdr, 16)) {
                int32_t cap, size; memcpy(&data, hdr, 8); memcpy(&cap, hdr + 8, 4); memcpy(&size, hdr + 12, 4);
                if (size >= 0 && size <= cap && r.tag < size && !read_string(data + (uint64_t)r.tag * f->tag_stride, r.tag_s, sizeof r.tag_s))
                    r.tag_s[0] = 0;
            }
            r.ok = 1;
        }
        if (!r.ok) { if (loaded == 1) { logj("\"event\":\"not_in_game\""); loaded = 0; } continue; }
        long long now = now_ms();
        int changed = r.speed != last.speed || r.paused != last.paused || r.tag != last.tag;
        long long every = polls < 300 ? 10000 : 60000; /* every 10 s for the first 10 minutes, then every minute */
        if (loaded != 1 || changed || now - last_log >= every) {
            logj("\"event\":\"%s\",\"date\":{\"raw\":%d,\"text\":\"%s\"},\"speed\":%d,\"paused\":%s,\"player_tag_id\":%d,"
                 "\"player\":\"%s\"",
                 loaded != 1 ? "game_loaded" : "read", r.date, r.date_s, r.speed, r.paused ? "true" : "false", r.tag,
                 r.tag_s);
            last_log = now;
        }
        loaded = 1;
        last = r;
    }
    free(cand); free(refs); scan_image_free(&im);
    return NULL;
}

__attribute__((constructor)) static void inproc_load(void) {
    const char *exe = _dyld_get_image_name(0);
    size_t n = exe ? strlen(exe) : 0;
    const char *tail = "/Contents/MacOS/hoi4";
    if (n < strlen(tail) || strcmp(exe + n - strlen(tail), tail)) return; /* not the game: inert */
    unsetenv("DYLD_INSERT_LIBRARIES"); /* programs the game starts do not inherit the module */
    const char *lp = getenv("HOI4_INPROC_LOG");
    if (!lp || !*lp || strlen(lp) >= sizeof g_log_path) return;
    snprintf(g_log_path, sizeof g_log_path, "%s", lp);
    g_log = open(g_log_path, O_WRONLY | O_CREAT | O_APPEND, 0600);
    if (g_log < 0) return;
    logj("\"event\":\"loaded\",\"module\":\"%s\",\"pid\":%d,\"exe\":\"%s\"", MODULE_VERSION, (int)getpid(), exe);
    if (killed()) { logj("\"event\":\"inert\",\"reason\":\"kill switch\""); return; }
    pthread_t th; pthread_attr_t a;
    pthread_attr_init(&a);
    pthread_attr_setdetachstate(&a, PTHREAD_CREATE_DETACHED);
    if (pthread_create(&th, &a, worker, NULL)) logj("\"event\":\"inert\",\"reason\":\"no thread\"");
    pthread_attr_destroy(&a);
}

__attribute__((destructor)) static void inproc_unload(void) {
    logj("\"event\":\"unload\"");
}
