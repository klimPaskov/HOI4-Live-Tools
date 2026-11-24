/* HOI4 Live Tools in-process module, macOS loader. STAGE 0 (read-only) + STAGE 1 (one command, opt-in).
 *
 * Loaded only when the project's own launcher starts the game with DYLD_INSERT_LIBRARIES set (opt-in, default
 * off; the game's files are never changed). What it does, and all it does:
 *   1. In the constructor: stays inert unless the host is the HOI4 executable and HOI4_INPROC_LOG names a log;
 *      removes DYLD_INSERT_LIBRARIES from the environment so programs the game starts do not load it; logs "loaded".
 *   2. On its own thread, after a delay: scans the game's image in memory with the same scanner the offline test
 *      runs on the executable file (core/scan.c, core/cmd.c), writes the capability table to the log and a cache.
 *   3. Waits for a loaded game: finds the game-state and idler roots by their RTTI dynamic type, logs reads (date,
 *      speed and pause, player) for comparison with the external runtime's hoi4_overview.
 *   4. STAGE 1, only when the launcher passes HOI4_INPROC_SOCK (a socket path in the run folder): serves the local
 *      channel (core/ipc.c). Posting is OFF unless the launcher ALSO passes HOI4_INPROC_POST=set_game_speed and every
 *      stage-1 capability resolved on this build. Then, per request, it queues ONE job onto the process main thread
 *      (the thread whose Cocoa event pump delivers the UI's input), where it checks the game's own thread rule
 *      (_ThreadForbidCount == 0), the session (single player), the sender Post will stamp and the authorisation the
 *      command requires, creates the command with the game's exported factory, fills its one field, and hands it to
 *      the game's Post exactly as the UI's speed handler does. The game stamps the sender; the module never writes
 *      game state. Every post is logged with its fields, result and timings.
 * Every read of game memory goes through vm_read_overwrite on its own task, which returns an error instead of
 * faulting. Game functions are called only on the main thread, only at addresses whose code pattern the scan proved,
 * on objects whose dynamic type was checked first. Kill switch: a file named <log>.off makes it inert (no new posts,
 * the channel closes) within about 2 s. */
#include "../core/caps.h"
#include "../core/ipc.h"
#include <dispatch/dispatch.h>
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

#define MODULE_VERSION "stage2-1"
static int g_log = -1;
static char g_log_path[1024];
static long g_log_bytes;
static pthread_mutex_t g_log_mu = PTHREAD_MUTEX_INITIALIZER;
static char g_sock_env[1024], g_post_env[256]; /* copied and removed from the environment in the constructor */

static long long now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_REALTIME, &t);
    return (long long)t.tv_sec * 1000 + t.tv_nsec / 1000000;
}
static long long mono_us(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return (long long)t.tv_sec * 1000000 + t.tv_nsec / 1000;
}

static void logj(const char *fmt, ...) {
    if (g_log < 0 || g_log_bytes > (8L << 20)) return; /* bounded: at most 8 MiB per game process */
    char body[40960], line[41100]; /* the capability line carries every stage-2 type (engineer 71: up to 32) */
    va_list ap; va_start(ap, fmt); vsnprintf(body, sizeof body, fmt, ap); va_end(ap);
    int n = snprintf(line, sizeof line, "{\"t\":%lld,%s}\n", now_ms(), body);
    pthread_mutex_lock(&g_log_mu);
    if (n > 0) { ssize_t w = write(g_log, line, (size_t)(n < (int)sizeof line ? n : (int)sizeof line - 1)); if (w > 0) g_log_bytes += w; }
    pthread_mutex_unlock(&g_log_mu);
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

/* Itanium RTTI: the dynamic type name of the object at obj (vptr[-1] = typeinfo, typeinfo+8 = name). */
static int dyn_type(uint64_t obj, char *out, size_t cap) {
    uint64_t vp, ti, nm; char b[64];
    out[0] = 0;
    if (!plausible(obj) || !sq(obj, &vp) || !plausible(vp) || !sq(vp - 8, &ti) || !plausible(ti) || !sq(ti + 8, &nm) ||
        !plausible(nm) || !sread(nm, b, sizeof b))
        return 0;
    b[sizeof b - 1] = 0;
    for (size_t i = 0; b[i]; i++) if ((unsigned char)b[i] < 0x20 || b[i] == '"' || b[i] == '\\') return 0;
    snprintf(out, cap, "%s", b);
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

/* ---- shared state (written by the worker before the channel starts, read by the channel and main-thread jobs) ---- */
static scan_image g_im;
static caps_table g_t;
static int64_t g_slide;
static uint64_t g_vt_gs, g_vt_idler;           /* slid */
static volatile uint64_t g_gs_glob, g_idler_glob; /* unslid globals, found by dynamic type */
static ipc_policy g_policy;

typedef struct {
    int ok, has_date;
    int32_t date, speed, tag;
    uint8_t paused;
    char date_s[24], tag_s[8];
} reads_t;

/* The stage-0 reads, plus the speed through the scanned Execute field when no per-build table exists. */
static int do_reads(reads_t *r) {
    memset(r, 0, sizeof *r);
    uint64_t gs, idler, vp;
    if (!g_gs_glob || !g_idler_glob) return 0;
    if (!sq(g_gs_glob + (uint64_t)g_slide, &gs) || !sq(gs, &vp) || vp != g_vt_gs) return 0;
    if (!sq(g_idler_glob + (uint64_t)g_slide, &idler) || !sq(idler, &vp) || vp != g_vt_idler) return 0;
    const caps_fields *f = g_t.fields;
    uint64_t speed_off = g_t.speed.ok ? g_t.speed.speed_field : (f ? f->speed : 0);
    if (!speed_off || !sread(gs + speed_off, &r->speed, 4) || r->speed < 0 || r->speed > 4) return 0;
    if (f) {
        if (!sread(gs + f->date, &r->date, 4) || !sread(idler + f->paused, &r->paused, 1) ||
            !sread(gs + f->player_tag, &r->tag, 4) || !date_text(r->date, r->date_s, sizeof r->date_s) || r->paused > 1 ||
            r->tag <= 0 || r->tag >= 4096)
            return 0;
        r->has_date = 1;
        uint64_t db, data; uint8_t hdr[16];
        if (sq(f->tag_db_global + (uint64_t)g_slide, &db) && sread(db, hdr, 16)) {
            int32_t cap, size; memcpy(&data, hdr, 8); memcpy(&cap, hdr + 8, 4); memcpy(&size, hdr + 12, 4);
            if (size >= 0 && size <= cap && r->tag < size && !read_string(data + (uint64_t)r->tag * f->tag_stride, r->tag_s, sizeof r->tag_s))
                r->tag_s[0] = 0;
        }
    }
    r->ok = 1;
    return 1;
}

static int reads_json(const reads_t *r, char *out, size_t n) {
    if (!r->ok) return snprintf(out, n, "\"in_game\":false"), 0;
    if (r->has_date)
        snprintf(out, n, "\"in_game\":true,\"date\":{\"raw\":%d,\"text\":\"%s\"},\"speed\":%d,\"paused\":%s,\"player_tag_id\":%d,\"player\":\"%s\"",
                 r->date, r->date_s, r->speed, r->paused ? "true" : "false", r->tag, r->tag_s);
    else
        snprintf(out, n, "\"in_game\":true,\"speed\":%d,\"date\":null,\"paused\":null,\"player\":null,\"reads\":\"speed only (no field table)\"", r->speed);
    return 1;
}

/* ---- the main-thread job: everything that calls the game happens here ---- */
typedef void *(*factory_fn)(void);
typedef void *(*getq_fn)(void *);
typedef unsigned char (*post_fn)(void *, void *, int);
typedef int *(*tlv_fn)(void);

typedef struct {
    int refs;                 /* 2: the waiting channel thread and the queued job; the last one out frees */
    int probe_only;
    int32_t speed; long id;
    long long queued_us, deadline_us;
    /* results */
    int ran, main_thread, forbid, requeues, posted, post_ret, refused, unexplained;
    int sender; int32_t speed_before;
    long long ran_us, post_us;
    char reason[200], queue_type[64], server_type[64];
    dispatch_semaphore_t done;
} job_t;

static void job_release(job_t *j) {
    if (__atomic_sub_fetch(&j->refs, 1, __ATOMIC_ACQ_REL) == 0) { dispatch_release(j->done); free(j); }
}

static void finish(job_t *j) { dispatch_semaphore_signal(j->done); job_release(j); }

static int code_ptr(uint64_t a) { /* inside the game image's __text (slid) */
    return a >= g_im.text_addr + (uint64_t)g_slide && a < g_im.text_addr + g_im.text_size + (uint64_t)g_slide;
}

static void run_job(void *ctx) {
    job_t *j = (job_t *)ctx;
    long long now = mono_us();
    j->ran = 1; j->ran_us = now - j->queued_us;
    j->main_thread = pthread_main_np() == 1;
    if (!j->main_thread) { snprintf(j->reason, sizeof j->reason, "job ran off the main thread"); j->unexplained = 1; finish(j); return; }
    if (killed()) { snprintf(j->reason, sizeof j->reason, "kill switch"); finish(j); return; }
    if (now > j->deadline_us) { snprintf(j->reason, sizeof j->reason, "expired before the main thread ran it (%lld ms)", j->ran_us / 1000); finish(j); return; }
    /* the game's own thread rule, read through its own accessor (proven by 6,4xx unanimous call sites) */
    uint64_t acc = g_t.forbid.accessor + (uint64_t)g_slide;
    int *pc = code_ptr(acc) ? ((tlv_fn)(uintptr_t)acc)() : NULL;
    int32_t fc = -1;
    if (!pc || !sread((uint64_t)(uintptr_t)pc, &fc, 4)) { snprintf(j->reason, sizeof j->reason, "thread-forbid count unreadable"); j->unexplained = 1; finish(j); return; }
    j->forbid = fc;
    if (fc != 0) {
        if (now + 3000 < j->deadline_us) { /* not now: try again at the next pump, until the deadline */
            j->requeues++;
            dispatch_after_f(dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_MSEC), dispatch_get_main_queue(), j, run_job);
            return;
        }
        snprintf(j->reason, sizeof j->reason, "game-state access forbidden on the main thread until the deadline (count %d)", fc);
        finish(j); return;
    }
    /* the game is loaded and the roots are what the scan says */
    uint64_t gs, idler, vp, q, server;
    if (!g_gs_glob || !g_idler_glob || !sq(g_gs_glob + (uint64_t)g_slide, &gs) || !sq(gs, &vp) || vp != g_vt_gs ||
        !sq(g_idler_glob + (uint64_t)g_slide, &idler) || !sq(idler, &vp) || vp != g_vt_idler) {
        snprintf(j->reason, sizeof j->reason, "no loaded game"); finish(j); return;
    }
    if (g_gs_glob != g_t.speed.gs_global) { snprintf(j->reason, sizeof j->reason, "Execute's game-state global differs from the root"); j->unexplained = 1; finish(j); return; }
    if (!sread(gs + g_t.speed.speed_field, &j->speed_before, 4)) { snprintf(j->reason, sizeof j->reason, "speed unreadable"); finish(j); return; }
    /* the session the UI posts to: idler->vfunc(queue slot)(), the same call the UI makes */
    uint64_t getter;
    if (!sq(vp + g_t.cap[CAP_QUEUE_ROOT].extra, &getter) || !code_ptr(getter)) { snprintf(j->reason, sizeof j->reason, "queue getter not game code"); j->unexplained = 1; finish(j); return; }
    q = (uint64_t)(uintptr_t)((getq_fn)(uintptr_t)getter)((void *)(uintptr_t)idler);
    if (!dyn_type(q, j->queue_type, sizeof j->queue_type) || strcmp(j->queue_type, "8CSession")) {
        snprintf(j->reason, sizeof j->reason, "the queue is not a CSession"); j->unexplained = 1; finish(j); return;
    }
    if (!sq(q + g_t.post.server_off, &server) || !dyn_type(server, j->server_type, sizeof j->server_type)) {
        snprintf(j->reason, sizeof j->reason, "no server on the session"); j->unexplained = 1; finish(j); return;
    }
    if (strcmp(j->server_type, "12CDummyServer")) { /* stage 1: single player only */
        snprintf(j->reason, sizeof j->reason, "not single player (server %s): stage 1 never posts in multiplayer", j->server_type);
        finish(j); return;
    }
    int32_t snd = -1;
    if (!sread(q + g_t.post.sender_src, &snd, 4)) { snprintf(j->reason, sizeof j->reason, "sender unreadable"); finish(j); return; }
    j->sender = snd;
    if ((uint32_t)snd != g_t.speed.required_sender) {
        snprintf(j->reason, sizeof j->reason, "Post would stamp sender %d; this command requires %u (Post would drop it)", snd, g_t.speed.required_sender);
        finish(j); return;
    }
    if (j->probe_only) { snprintf(j->reason, sizeof j->reason, "probe: every check passed, nothing posted"); finish(j); return; }
    /* create with the game's factory, check what it built, fill the one field the UI fills, post as the UI does */
    uint64_t fac = g_t.speed.factory + (uint64_t)g_slide, post = g_t.post.post + (uint64_t)g_slide;
    if (!code_ptr(fac) || !code_ptr(post)) { snprintf(j->reason, sizeof j->reason, "factory/post not game code"); j->unexplained = 1; finish(j); return; }
    void *cmd = ((factory_fn)(uintptr_t)fac)();
    uint64_t cvp = 0; int32_t csender = 0, cval = -1;
    if (!cmd || !sq((uint64_t)(uintptr_t)cmd, &cvp) || cvp != g_t.speed.vtable + (uint64_t)g_slide ||
        !sread((uint64_t)(uintptr_t)cmd + 0xc, &csender, 4) || csender != -1 ||
        !sread((uint64_t)(uintptr_t)cmd + g_t.speed.payload_off, &cval, 4) || cval != 0) {
        /* never post or call anything on an object we do not recognise; 40 bytes are left unfreed */
        snprintf(j->reason, sizeof j->reason, "the factory built an unexpected object"); j->unexplained = 1; finish(j); return;
    }
    *(int32_t *)((char *)cmd + g_t.speed.payload_off) = j->speed; /* our own fresh command object, not game state */
    long long t0 = mono_us();
    unsigned char r = ((post_fn)(uintptr_t)post)((void *)(uintptr_t)q, cmd, 0); /* Post owns cmd from here */
    j->post_us = mono_us() - t0;
    j->posted = 1; j->post_ret = r != 0;
    if (!r) { snprintf(j->reason, sizeof j->reason, "Post returned false although every check passed"); j->unexplained = 1; }
    finish(j);
}

/* Queue a job on the main thread and wait for it (at most 1.5 s; the job's own deadline is 1 s). */
static job_t *run_on_main(int probe_only, int32_t speed, long id) {
    job_t *j = (job_t *)calloc(1, sizeof *j);
    if (!j) return NULL;
    j->refs = 2; j->probe_only = probe_only; j->speed = speed; j->id = id;
    j->done = dispatch_semaphore_create(0);
    j->queued_us = mono_us(); j->deadline_us = j->queued_us + 1000000;
    dispatch_async_f(dispatch_get_main_queue(), j, run_job);
    if (dispatch_semaphore_wait(j->done, dispatch_time(DISPATCH_TIME_NOW, 1500 * NSEC_PER_MSEC))) {
        snprintf(j->reason, sizeof j->reason, "the main thread did not run the job within 1.5 s");
        /* the job still holds its reference; when it runs it sees its deadline passed and does nothing */
        return j;
    }
    return j;
}

static int be_hello(char *out, size_t n) {
    size_t at = (size_t)snprintf(out, n, "\"module\":\"%s\",\"uuid\":\"%s\",\"capabilities\":{", MODULE_VERSION, g_im.uuid);
    for (int i = 0; i < CAP_COUNT && at < n; i++)
        at += (size_t)snprintf(out + at, n - at, "%s\"%s\":%s", i ? "," : "", g_t.cap[i].name, g_t.cap[i].ok ? "true" : "false");
    if (at < n)
        snprintf(out + at, n - at, "},\"speed\":{\"type_id\":%u,\"payload_off\":%u,\"range_max\":%u,\"field_id\":%u,\"speed_field\":%u},"
                 "\"game_loaded\":%s", g_t.speed.type_id, g_t.speed.payload_off, g_t.speed.range_max, g_t.speed.field_id,
                 g_t.speed.speed_field, g_gs_glob && g_idler_glob ? "true" : "false");
    return 1;
}

static int be_read(char *out, size_t n) {
    reads_t r; long long t0 = mono_us();
    do_reads(&r);
    char b[400]; reads_json(&r, b, sizeof b);
    snprintf(out, n, "%s,\"read_us\":%lld", b, mono_us() - t0);
    return r.ok;
}

static int be_probe(char *out, size_t n) {
    job_t *j = run_on_main(1, 0, 0);
    if (!j) return 0;
    int ok = j->ran && !j->unexplained && !strncmp(j->reason, "probe:", 6);
    snprintf(out, n, "\"ran\":%s,\"main_thread\":%s,\"forbid\":%d,\"requeues\":%d,\"wait_ms\":%.2f,\"queue_type\":\"%s\","
             "\"server_type\":\"%s\",\"sender\":%d,\"speed\":%d,\"would_post\":%s,\"reason\":\"%s\"",
             j->ran ? "true" : "false", j->main_thread ? "true" : "false", j->forbid, j->requeues, j->ran_us / 1000.0,
             j->queue_type, j->server_type, j->sender, j->speed_before, ok ? "true" : "false", j->reason);
    logj("\"event\":\"probe\",%s", out);
    job_release(j);
    return ok;
}

static int be_post_speed(int64_t value, long id, char *out, size_t n, int *unexplained) {
    long long t0 = mono_us();
    job_t *j = run_on_main(0, (int32_t)value, id);
    if (!j) return 0;
    int ok = 0; long long rb_us = -1; int32_t after = -1;
    reads_t r0; do_reads(&r0);
    if (j->posted && j->post_ret) { /* read back: the game executes the command at its next command step */
        long long start = mono_us();
        while (mono_us() - start < 3000000) {
            reads_t r;
            if (do_reads(&r) && r.speed == (int32_t)value) { rb_us = mono_us() - t0; after = r.speed; break; }
            usleep(1000);
        }
        ok = rb_us >= 0;
        if (!ok) {
            reads_t r; do_reads(&r); after = r.speed;
            snprintf(j->reason, sizeof j->reason, "posted; the game had not executed it after 3 s%s",
                     r.ok && r.has_date && r.paused ? " (game paused)" : "");
            if (!(r.ok && r.has_date && r.paused)) j->unexplained = 1;
        }
    }
    *unexplained = j->unexplained;
    const char *result = !j->ran ? "not_run" : j->posted ? (j->post_ret ? (ok ? "executed" : "posted_not_executed") : "dropped_by_post")
                                                         : "refused";
    snprintf(out, n, "\"cmd\":\"set_game_speed\",\"type_id\":%u,\"fields\":{\"speed\":%lld},\"result\":\"%s\",\"reason\":\"%s\","
             "\"speed_before\":%d,\"speed_after\":%d,\"main_wait_ms\":%.2f,\"requeues\":%d,\"forbid\":%d,\"post_us\":%lld,"
             "\"readback_ms\":%.2f,\"total_ms\":%.2f,\"paused\":%s",
             g_t.speed.type_id, (long long)value, result, j->reason, j->speed_before, after, j->ran_us / 1000.0, j->requeues,
             j->forbid, j->post_us, rb_us >= 0 ? rb_us / 1000.0 : -1.0, (mono_us() - t0) / 1000.0,
             r0.ok && r0.has_date ? (r0.paused ? "true" : "false") : "null");
    logj("\"event\":\"post\",\"id\":%ld,%s", id, out);
    job_release(j);
    return ok;
}


/* ======================= STAGE 2: the families, posted the way the UI posts them =======================
 * One request = one family op over up to 32 own objects, run as ONE main-thread job (one command per object, as the
 * UI's helpers do: N wings or N lines = N posts in the same frame, microseconds each). For each object, before
 * anything is created: the object pointer must round-trip through the game's own typed resolver (resolve(handle(obj))
 * == obj: a live object of the command's class) and the game's own owner getter must return the player's tag (the
 * field the game's per-frame command gate compares). The command is made by the game's factory (and, for container
 * payloads, the UI's own constructor), checked, then the game's own "can do" check decides (refused: the module
 * deletes it with the command's own destructor, as the UI does), then Post(session, cmd, 0) exactly as the UI. */
typedef unsigned char (*valid_fn2)(void *, int);
typedef void (*delete_fn2)(void *);
typedef void *(*resolve_fn2)(const void *);
typedef int32_t (*owner_fn2)(void *);
typedef void (*ctor_wing_i32_u8)(void *, void *, int32_t, int);
typedef void (*ctor_wing_i32_ptr)(void *, void *, int32_t, void *);
typedef void (*ctor_obj)(void *, void *);
typedef void (*ctor_tagp_obj)(void *, const int32_t *, void *);
typedef void (*ctor_obj_obj2)(void *, void *, void *);
typedef struct { void **data; int32_t capacity, size; } pdx_array;
typedef void (*ctor_array)(void *, const pdx_array *);
typedef void (*ctor_addline)(void *, const int32_t *, void *, void *, int);

#define J2_MAX 32
enum { R_NONE = 0, R_POSTED, R_INVALID, R_NOT_OWN, R_BAD_OBJ, R_BAD_OBJ2, R_DROPPED, R_UNEXPECTED, R_SKIPPED };
static const char *const R_NAMES[] = {"none", "posted", "refused_by_game_check", "not_own", "not_a_live_object",
                                      "second_object_invalid", "dropped_by_post", "unexpected_object", "skipped"};

typedef struct {
    int refs;
    const cmd_type *ct;
    int n; uint64_t obj[J2_MAX]; int64_t val[J2_MAX]; uint64_t obj2; int64_t flag; long id;
    long long queued_us, deadline_us;
    int ran, main_thread, forbid, requeues, unexplained, posted, refused;
    int32_t player_tag, region_id;
    int res[J2_MAX]; int32_t before[J2_MAX];
    long long ran_us, post_us;
    char reason[200], queue_type[64], server_type[64];
    dispatch_semaphore_t done;
} job2_t;

static void job2_release(job2_t *j) {
    if (__atomic_sub_fetch(&j->refs, 1, __ATOMIC_ACQ_REL) == 0) { dispatch_release(j->done); free(j); }
}
static void finish2(job2_t *j) { dispatch_semaphore_signal(j->done); job2_release(j); }

typedef void *(*dyncast_fn)(const void *, const void *, const void *, long);
static dyncast_fn g_dyncast;

/* fault-free read of a handle at obj+off; then the game's typed resolver must give back obj. For a type whose
 * authorisation is the country tag (AUTH_TAG), the object is checked the way its "can do" check reads it: the generic
 * lookup gives it back and the C++ cast to the class the check casts to accepts it. */
static int roundtrip(const cmd_type *ct, uint64_t obj, uint64_t *handle) {
    const stage2_common *c = &g_t.st2.common;
    uint64_t h;
    if (ct->spec->auth_kind == AUTH_TAG) {
        uint64_t lk = ct->lookup_fn + (uint64_t)g_slide;
        if (!g_dyncast) g_dyncast = (dyncast_fn)dlsym(RTLD_DEFAULT, "__dynamic_cast");
        if (!g_dyncast || !ct->ti_dst || !plausible(obj) || !sq(obj + c->cref_handle_off, &h) || h == 0 || !code_ptr(lk)) return 0;
        if ((uint64_t)(uintptr_t)((resolve_fn2)(uintptr_t)lk)(&h) != obj) return 0;
        *handle = h;
        return (uint64_t)(uintptr_t)g_dyncast((void *)(uintptr_t)obj, (void *)(uintptr_t)(ct->ti_src + (uint64_t)g_slide),
                                              (void *)(uintptr_t)(ct->ti_dst + (uint64_t)g_slide), -1) == obj;
    }
    if (!plausible(obj) || !sq(obj + ct->target_hint + c->cref_handle_off, &h) || h == 0) return 0;
    uint64_t rf = ct->resolve_fn + (uint64_t)g_slide;
    if (!code_ptr(rf)) return 0;
    void *back = ((resolve_fn2)(uintptr_t)rf)(&h);
    *handle = h;
    return (uint64_t)(uintptr_t)back == obj;
}

static void run_job2(void *ctx) {
    job2_t *j = (job2_t *)ctx;
    const stage2_common *c = &g_t.st2.common;
    const cmd_type *ct = j->ct;
    long long now = mono_us();
    j->ran = 1; j->ran_us = now - j->queued_us;
    j->main_thread = pthread_main_np() == 1;
    if (!j->main_thread) { snprintf(j->reason, sizeof j->reason, "job ran off the main thread"); j->unexplained = 1; finish2(j); return; }
    if (killed()) { snprintf(j->reason, sizeof j->reason, "kill switch"); finish2(j); return; }
    if (now > j->deadline_us) { snprintf(j->reason, sizeof j->reason, "expired before the main thread ran it"); finish2(j); return; }
    uint64_t acc = g_t.forbid.accessor + (uint64_t)g_slide;
    int *pc = code_ptr(acc) ? ((tlv_fn)(uintptr_t)acc)() : NULL;
    int32_t fc = -1;
    if (!pc || !sread((uint64_t)(uintptr_t)pc, &fc, 4)) { snprintf(j->reason, sizeof j->reason, "thread-forbid count unreadable"); j->unexplained = 1; finish2(j); return; }
    j->forbid = fc;
    uint64_t gs, idler, vp, q, server;
    if (!g_gs_glob || !g_idler_glob || !sq(g_gs_glob + (uint64_t)g_slide, &gs) || !sq(gs, &vp) || vp != g_vt_gs ||
        !sq(g_idler_glob + (uint64_t)g_slide, &idler) || !sq(idler, &vp) || vp != g_vt_idler) {
        snprintf(j->reason, sizeof j->reason, "no loaded game"); finish2(j); return;
    }
    uint64_t getter;
    if (!sq(vp + g_t.cap[CAP_QUEUE_ROOT].extra, &getter) || !code_ptr(getter)) { snprintf(j->reason, sizeof j->reason, "queue getter not game code"); j->unexplained = 1; finish2(j); return; }
    q = (uint64_t)(uintptr_t)((getq_fn)(uintptr_t)getter)((void *)(uintptr_t)idler);
    if (!dyn_type(q, j->queue_type, sizeof j->queue_type) || strcmp(j->queue_type, "8CSession")) { snprintf(j->reason, sizeof j->reason, "the queue is not a CSession"); j->unexplained = 1; finish2(j); return; }
    uint8_t gate = 0, executing = 1;
    if (!sread(q + c->gate_flag_off, &gate, 1) || !sread(q + c->exec_flag_off, &executing, 1)) { snprintf(j->reason, sizeof j->reason, "session flags unreadable"); finish2(j); return; }
    if (fc != 0 || executing) { /* not a point where the UI could post: try again at the next pump, until the deadline */
        if (now + 3000 < j->deadline_us) { j->requeues++; dispatch_after_f(dispatch_time(DISPATCH_TIME_NOW, 2 * NSEC_PER_MSEC), dispatch_get_main_queue(), j, run_job2); return; }
        snprintf(j->reason, sizeof j->reason, "%s until the deadline", fc ? "game-state access forbidden" : "the session was executing commands");
        finish2(j); return;
    }
    if (!sq(q + g_t.post.server_off, &server) || !dyn_type(server, j->server_type, sizeof j->server_type)) { snprintf(j->reason, sizeof j->reason, "no server on the session"); j->unexplained = 1; finish2(j); return; }
    if (strcmp(j->server_type, "12CDummyServer")) { snprintf(j->reason, sizeof j->reason, "not single player (server %s): stage 2 never posts in multiplayer", j->server_type); finish2(j); return; }
    if (!gate) { snprintf(j->reason, sizeof j->reason, "the session's command gate is closed (no country played): Post would drop"); finish2(j); return; }
    if (!sread(gs + c->player_tag_off, &j->player_tag, 4) || j->player_tag <= 0) { snprintf(j->reason, sizeof j->reason, "no player country"); finish2(j); return; }
    int32_t snd = -1;
    if (!sread(q + g_t.post.sender_src, &snd, 4) || snd < 0) { snprintf(j->reason, sizeof j->reason, "sender unreadable"); finish2(j); return; }
    /* the second object, by the spec's kind: an air base (transfer) or a strategic region (enable_mission) checked by
     * its exact vtable; a region given by id (no object: a region with no own base, or 0 = hold) looked up the way the
     * command's own check reads it; an army (attach) whose handle must resolve to it and which the game's own cast
     * must accept as a COrdersGroup */
    const char *op = ct->spec->op;
    uint64_t h2 = 0;
    int k2 = ct->spec->obj2;
    if (k2 == OBJ2_AIR_BASE || k2 == OBJ2_REGION) {
        int is_base = k2 == OBJ2_AIR_BASE;
        uint64_t want = (is_base ? c->vt_base : c->vt_region) + (uint64_t)g_slide, v2 = 0;
        if (!plausible(j->obj2) || !sq(j->obj2, &v2) || v2 != want) { for (int i = 0; i < j->n; i++) j->res[i] = R_BAD_OBJ2; snprintf(j->reason, sizeof j->reason, "obj2 is not a live %s", is_base ? "CAirBase" : "CStrategicRegion"); finish2(j); return; }
        if (is_base) {
            uint64_t lk = ct->lookup_fn + (uint64_t)g_slide;
            if (!sq(j->obj2 + c->cref_handle_off, &h2) || !code_ptr(lk) || (uint64_t)(uintptr_t)((resolve_fn2)(uintptr_t)lk)(&h2) != j->obj2) {
                for (int i = 0; i < j->n; i++) j->res[i] = R_BAD_OBJ2; snprintf(j->reason, sizeof j->reason, "the base's handle does not resolve to it"); finish2(j); return;
            }
        } else if (!sread(j->obj2 + c->region_id_off, &j->region_id, 4) || j->region_id < 0) {
            for (int i = 0; i < j->n; i++) j->res[i] = R_BAD_OBJ2; snprintf(j->reason, sizeof j->reason, "region id unreadable"); finish2(j); return;
        }
    } else if (k2 == OBJ2_REGION_ID) { /* by id: j->region_id was set from the request (0 = hold: no region) */
        if (j->region_id > 0) {
            int32_t cnt = 0; uint64_t arr = 0, reg = 0, v2 = 0; int32_t rid = -1;
            if (!c->regions_arr_off || !sread(gs + c->regions_cnt_off, &cnt, 4) || j->region_id >= cnt ||
                !sq(gs + c->regions_arr_off, &arr) || !plausible(arr) || !sq(arr + 8 * (uint64_t)j->region_id, &reg) ||
                !plausible(reg) || !sq(reg, &v2) || v2 != c->vt_region + (uint64_t)g_slide ||
                !sread(reg + c->region_id_off, &rid, 4) || rid != j->region_id) {
                for (int i = 0; i < j->n; i++) j->res[i] = R_BAD_OBJ2;
                snprintf(j->reason, sizeof j->reason, "region %d is not a live strategic region (%d regions)", j->region_id, cnt);
                finish2(j); return;
            }
        }
    } else if (k2 == OBJ2_ORDERS_GROUP) {
        typedef void *(*dyncast_fn)(const void *, const void *, const void *, long);
        static dyncast_fn dc;
        if (!dc) dc = (dyncast_fn)dlsym(RTLD_DEFAULT, "__dynamic_cast");
        uint64_t lk = ct->lookup_fn + (uint64_t)g_slide;
        if (!dc || !plausible(j->obj2) || !sq(j->obj2 + c->cref_handle_off, &h2) || !h2 || !code_ptr(lk) ||
            (uint64_t)(uintptr_t)((resolve_fn2)(uintptr_t)lk)(&h2) != j->obj2 ||
            (uint64_t)(uintptr_t)dc((void *)(uintptr_t)j->obj2, (void *)(uintptr_t)(c->ti_reference + (uint64_t)g_slide),
                                    (void *)(uintptr_t)(c->ti_orders_group + (uint64_t)g_slide), -1) != j->obj2) {
            for (int i = 0; i < j->n; i++) j->res[i] = R_BAD_OBJ2;
            snprintf(j->reason, sizeof j->reason, "obj2 is not a live army (COrdersGroup)"); finish2(j); return;
        }
    }
    uint64_t fac = ct->factory + (uint64_t)g_slide, post = g_t.post.post + (uint64_t)g_slide, nullref = 0;
    if (!code_ptr(fac) || !code_ptr(post) || !sq(c->null_ref_global + (uint64_t)g_slide, &nullref)) { snprintf(j->reason, sizeof j->reason, "factory/post not game code"); j->unexplained = 1; finish2(j); return; }
    long long tp = 0;
    /* every object first: a live object of the command's class (the game's typed resolver gives it back) owned by the
     * player's country (the game's owner getter) */
    uint64_t hs[J2_MAX] = {0};
    int good = 0;
    for (int i = 0; i < j->n; i++) {
        uint64_t owner_f = ct->owner_fn + (uint64_t)g_slide;
        if (!roundtrip(ct, j->obj[i], &hs[i])) { j->res[i] = R_BAD_OBJ; j->refused++; continue; }
        /* AUTH_TAG: the command names the player's own tag; whether the country may use the object is the game's check */
        if (ct->spec->auth_kind != AUTH_TAG &&
            (!code_ptr(owner_f) || ((owner_fn2)(uintptr_t)owner_f)((void *)(uintptr_t)j->obj[i]) != j->player_tag)) { j->res[i] = R_NOT_OWN; j->refused++; continue; }
        good++;
    }
    /* one command for the whole group (merge) only when every object passed: a group is never posted in part */
    if (ct->spec->one_cmd && good != j->n) {
        for (int i = 0; i < j->n; i++) if (j->res[i] == R_NONE) { j->res[i] = R_SKIPPED; }
        snprintf(j->reason, sizeof j->reason, "a group command is posted whole or not at all");
        j->post_us = 0; finish2(j); return;
    }
    int ncmd = ct->spec->one_cmd ? 1 : j->n;
    for (int ci = 0; ci < ncmd; ci++) {
        int i = ci; /* the object this command is for (the first of the group for one_cmd) */
        if (!ct->spec->one_cmd && j->res[i] != R_NONE) continue;
        uint64_t h = hs[i];
        if (!strcmp(op, "production.add_factories")) sread(j->obj[i] + c->line_factories_off, &j->before[i], 4);
        char *cmd = (char *)((factory_fn)(uintptr_t)fac)();
        uint64_t cvp = 0; int32_t cs = 0;
        if (!cmd || !sq((uint64_t)(uintptr_t)cmd, &cvp) || cvp != ct->vtable + (uint64_t)g_slide || !sread((uint64_t)(uintptr_t)cmd + 0xc, &cs, 4) || cs != -1) {
            j->res[i] = R_UNEXPECTED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "the factory built an unexpected object"); break;
        }
        int ok = 1;
        if (ct->spec->build == BUILD_FIELDS) {
            for (int k = 0; k < ct->spec->nf && ok; k++) { /* every field still holds the factory's default */
                const cmd_field *f = &ct->spec->f[k];
                uint64_t cur = 0;
                if (f->kind == FK_REF) ok = sq((uint64_t)(uintptr_t)cmd + f->off, &cur) && cur == nullref;
                else if (f->kind == FK_I32) { int32_t v = 1; ok = sread((uint64_t)(uintptr_t)cmd + f->off, &v, 4) && v == f->def; }
                else ok = 0;
            }
            if (ok) { /* fill: our own fresh command object, never game state */
                for (int k = 0; k < ct->spec->nf; k++) {
                    const cmd_field *f = &ct->spec->f[k];
                    if (f->kind == FK_REF) *(uint64_t *)(cmd + f->off) = k == 0 ? h : h2;
                    else *(int32_t *)(cmd + f->off) = (int32_t)j->val[i];
                }
            }
        } else { /* the UI's own constructor, on the fresh object whose containers are still empty */
            uint64_t data = 1; int32_t size = 1;
            if (ct->spec->ctor == CTOR_ADDLINE) /* no list: the variant's handle field still holds the empty handle */
                ok = sq((uint64_t)(uintptr_t)cmd + ct->spec->f[1].off, &data) && data == nullref;
            else
                ok = sq((uint64_t)(uintptr_t)cmd + ct->spec->f[0].off, &data) && data == 0 &&
                     sread((uint64_t)(uintptr_t)cmd + ct->spec->f[0].count_off, &size, 4) && size == 0;
            uint64_t ctor = ct->ctor + (uint64_t)g_slide;
            void *obj = (void *)(uintptr_t)j->obj[i];
            if (ok && code_ptr(ctor)) {
                switch (ct->spec->ctor) {
                case CTOR_WING_I32_U8: ((ctor_wing_i32_u8)(uintptr_t)ctor)(cmd, obj, (int32_t)j->val[i], j->flag ? 1 : 0); break;
                case CTOR_WING_I32_NULL: ((ctor_wing_i32_ptr)(uintptr_t)ctor)(cmd, obj, j->region_id, NULL); break;
                case CTOR_OBJ: ((ctor_obj)(uintptr_t)ctor)(cmd, obj); break;
                case CTOR_TAGP_OBJ: ((ctor_tagp_obj)(uintptr_t)ctor)(cmd, &j->player_tag, obj); break;
                case CTOR_OBJ_OBJ2: ((ctor_obj_obj2)(uintptr_t)ctor)(cmd, obj, (void *)(uintptr_t)j->obj2); break;
                case CTOR_ADDLINE: ((ctor_addline)(uintptr_t)ctor)(cmd, &j->player_tag, obj, NULL, j->flag ? 1 : 0); break;
                case CTOR_ARRAY: {
                    pdx_array a = {(void **)(uintptr_t)j->obj, j->n, j->n}; /* the game's CPdxArray<T *> {data, capacity, size} */
                    ((ctor_array)(uintptr_t)ctor)(cmd, &a); break;
                }
                default: ok = 0;
                }
            } else ok = 0;
            if (ok && ct->spec->ctor == CTOR_ADDLINE) { /* tag = player, variant = this object's handle, no organisation, the flag */
                int32_t tg = 0; uint64_t hv = 0, ho = 0; uint8_t top = 9;
                ok = sq((uint64_t)(uintptr_t)cmd, &cvp) && cvp == ct->vtable + (uint64_t)g_slide &&
                     sread((uint64_t)(uintptr_t)cmd + 0x24, &tg, 4) && tg == j->player_tag &&
                     sq((uint64_t)(uintptr_t)cmd + ct->spec->f[1].off, &hv) && hv == h &&
                     sq((uint64_t)(uintptr_t)cmd + 0x30, &ho) && ho == nullref &&
                     sread((uint64_t)(uintptr_t)cmd + 0x38, &top, 1) && top == (j->flag ? 1 : 0);
                if (!ok) { j->res[i] = R_UNEXPECTED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "the constructor built an unexpected payload"); break; }
            } else if (ok) { /* what the game built must be exactly what was asked: the handles in order, then the second field */
                uint64_t d2 = 0; int32_t s2 = 0;
                int want_n = ct->spec->one_cmd ? j->n : 1;
                ok = sq((uint64_t)(uintptr_t)cmd, &cvp) && cvp == ct->vtable + (uint64_t)g_slide &&
                     sq((uint64_t)(uintptr_t)cmd + ct->spec->f[0].off, &d2) && sread((uint64_t)(uintptr_t)cmd + ct->spec->f[0].count_off, &s2, 4) &&
                     s2 == want_n;
                for (int e = 0; ok && e < want_n; e++) { uint64_t ee = 0; ok = sq(d2 + 8 * (uint64_t)e, &ee) && ee == hs[ct->spec->one_cmd ? e : i]; }
                if (ok && ct->spec->nf > 1) {
                    const cmd_field *f1 = &ct->spec->f[1];
                    int32_t v2 = 0; uint64_t r2 = 0;
                    switch (ct->spec->ctor) {
                    case CTOR_WING_I32_U8: ok = sread((uint64_t)(uintptr_t)cmd + f1->off, &v2, 4) && v2 == (int32_t)j->val[i]; break;
                    case CTOR_WING_I32_NULL: ok = sread((uint64_t)(uintptr_t)cmd + f1->off, &v2, 4) && v2 == j->region_id; break;
                    case CTOR_TAGP_OBJ: ok = sread((uint64_t)(uintptr_t)cmd + f1->off, &v2, 4) && v2 == j->player_tag; break;
                    case CTOR_OBJ_OBJ2: ok = sq((uint64_t)(uintptr_t)cmd + f1->off, &r2) && r2 == h2; break;
                    default: ok = 0;
                    }
                }
                if (!ok) { j->res[i] = R_UNEXPECTED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "the constructor built an unexpected payload"); break; }
            }
        }
        if (!ok) { j->res[i] = R_UNEXPECTED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "the factory's defaults differ from the scan"); break; }
        void **vt = *(void ***)cmd;
        unsigned char valid = ((valid_fn2)vt[c->valid_slot / 8])(cmd, 0); /* the game's own "can do" check, as the UI */
        int first = ct->spec->one_cmd ? 0 : i, last = ct->spec->one_cmd ? j->n - 1 : i;
        if (!valid) { ((delete_fn2)vt[c->delete_slot / 8])(cmd); for (int e = first; e <= last; e++) { j->res[e] = R_INVALID; j->refused++; } continue; }
        long long t0 = mono_us();
        unsigned char r = ((post_fn)(uintptr_t)post)((void *)(uintptr_t)q, cmd, 0); /* Post owns cmd from here */
        tp += mono_us() - t0;
        if (r) { for (int e = first; e <= last; e++) { j->res[e] = R_POSTED; j->posted++; } }
        else { j->res[i] = R_DROPPED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "Post dropped a command every check passed"); break; }
    }
    j->post_us = tp;
    finish2(j);
}

static int parse_list_u64(const char *s, uint64_t *out, int max) {
    int n = 0;
    while (*s && n < max) {
        char *e; unsigned long long v = strtoull(s, &e, 16);
        if (e == s) return -1;
        out[n++] = (uint64_t)v;
        if (*e == ',') e++; else if (*e) return -1;
        s = e;
    }
    return *s ? -1 : n;
}
static int parse_list_i64(const char *s, int64_t *out, int max) {
    int n = 0;
    while (*s && n < max) {
        char *e; long long v = strtoll(s, &e, 10);
        if (e == s) return -1;
        out[n++] = v;
        if (*e == ',') e++; else if (*e) return -1;
        s = e;
    }
    return *s ? -1 : n;
}

static int be_post2(const char *op, const char *objs, const char *vals, const char *obj2, int64_t flag, long id, char *out, size_t n,
                    int *unexplained) {
    const cmd_type *ct = stage2_find(&g_t.st2, op);
    if (!ct || !ct->ok) { snprintf(out, n, "\"op\":\"%s\",\"result\":\"refused\",\"reason\":\"type not resolved on this build\"", op); logj("\"event\":\"post2\",\"id\":%ld,%s", id, out); return 0; }
    job2_t *j = (job2_t *)calloc(1, sizeof *j);
    if (!j) return 0;
    j->refs = 2; j->ct = ct; j->id = id; j->flag = flag;
    j->n = parse_list_u64(objs, j->obj, J2_MAX);
    int nv = vals && *vals ? parse_list_i64(vals, j->val, J2_MAX) : 0;
    const char *why = NULL;
    if (j->n < 1) why = "objs: comma-separated hex addresses";
    else if (!strcmp(op, "production.add_factories")) {
        if (nv != j->n) why = "vals: one factory delta per line";
        else for (int i = 0; i < j->n; i++) if (j->val[i] == 0 || j->val[i] < -150 || j->val[i] > 150) why = "a delta is 0 or outside -150..150";
    } else if (!strcmp(op, "air.set_mission")) {
        if (nv != 1 || j->val[0] <= 0 || j->val[0] > 0x40000000 || (j->val[0] & (j->val[0] - 1))) why = "vals: one mission mask with exactly one bit";
        else { for (int i = 1; i < j->n; i++) j->val[i] = j->val[0]; if (flag != 0 && flag != 1) why = "flag: 1 enable, 0 disable"; }
    } else if (!strcmp(op, "production2.priority")) {
        if (nv != j->n) why = "vals: one target place per line (0 = top)";
        else for (int i = 0; i < j->n; i++) if (j->val[i] < 0 || j->val[i] > 2047) why = "a place is outside 0..2047";
    } else if (!strcmp(op, "production2.add_line")) {
        if (nv) why = "add_line takes no vals (flag 1 = at the top)";
        else if (flag != 0 && flag != 1) why = "flag: 1 at the top, 0 at the bottom";
    } else if (!strcmp(op, "airwing.split")) {
        if (nv != j->n) why = "vals: one aircraft count per wing (the aircraft that leave for the new wing)";
        else for (int i = 0; i < j->n; i++) if (j->val[i] < 1 || j->val[i] > 1000) why = "an aircraft count is outside 1..1000";
    } else if (!strcmp(op, "airwing.region")) {
        /* a region by id (no own base there) or 0 = hold: the UI's region click and its Hold button post this command */
        if (nv != 1 || j->val[0] < 0 || j->val[0] > 100000) why = "vals: one strategic region id (0 = hold)";
        else { j->region_id = (int32_t)j->val[0]; for (int i = 1; i < j->n; i++) j->val[i] = j->val[0]; }
    } else if (nv) why = "this op takes no vals";
    if (!why && !strcmp(op, "airwing.merge") && j->n < 2) why = "a merge takes at least two wings";
    if (!why && (!strcmp(op, "air.transfer") || !strcmp(op, "air.enable_mission") || !strcmp(op, "airwing.attach"))) {
        uint64_t o2[1];
        if (parse_list_u64(obj2 ? obj2 : "", o2, 1) != 1) why = "obj2: one hex address"; else j->obj2 = o2[0];
    }
    for (int a = 0; !why && a < j->n; a++) for (int b = a + 1; b < j->n; b++) if (j->obj[a] == j->obj[b]) why = "an object is listed twice (no duplicate orders)";
    if (why) { snprintf(out, n, "\"op\":\"%s\",\"result\":\"refused_by_policy\",\"reason\":\"%s\"", op, why); free(j); logj("\"event\":\"post2\",\"id\":%ld,%s", id, out); return 0; }
    long long t0 = mono_us();
    j->done = dispatch_semaphore_create(0);
    j->queued_us = t0; j->deadline_us = t0 + 1000000;
    dispatch_async_f(dispatch_get_main_queue(), j, run_job2);
    if (dispatch_semaphore_wait(j->done, dispatch_time(DISPATCH_TIME_NOW, 1500 * NSEC_PER_MSEC))) {
        snprintf(out, n, "\"op\":\"%s\",\"result\":\"not_run\",\"reason\":\"the main thread did not run the job within 1.5 s\"", op);
        logj("\"event\":\"post2\",\"id\":%ld,%s", id, out);
        return 0; /* the job keeps its reference; it expires by its deadline and posts nothing */
    }
    /* read-back the module can do itself: a line's factories (field proven by the game's own add-factories check) */
    int executed = 0, pending = 0; long long rb_us = -1;
    if (!strcmp(op, "production.add_factories") && j->posted) {
        long long start = mono_us();
        while (mono_us() - start < 3000000) {
            executed = 0; pending = 0;
            for (int i = 0; i < j->n; i++) {
                if (j->res[i] != R_POSTED) continue;
                int32_t now_f = -1;
                if (sread(j->obj[i] + g_t.st2.common.line_factories_off, &now_f, 4) && now_f == j->before[i] + (int32_t)j->val[i]) executed++; else pending++;
            }
            if (!pending) { rb_us = mono_us() - t0; break; }
            usleep(1000);
        }
        if (pending) {
            reads_t r; do_reads(&r);
            if (!(r.ok && r.has_date && r.paused)) { j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "posted; %d of %d not executed after 3 s in a running game", pending, j->posted); }
            else snprintf(j->reason, sizeof j->reason, "posted; %d not executed yet (game paused)", pending);
        }
    }
    *unexplained = j->unexplained;
    size_t at = (size_t)snprintf(out, n, "\"op\":\"%s\",\"type_id\":%u,\"objects\":%d,\"posted\":%d,\"refused\":%d,\"executed\":%d,"
                                 "\"result\":\"%s\",\"reason\":\"%s\",\"main_wait_ms\":%.2f,\"requeues\":%d,\"forbid\":%d,"
                                 "\"post_us_total\":%lld,\"readback_ms\":%.2f,\"total_ms\":%.2f,\"player_tag_id\":%d,\"per_object\":[",
                                 op, ct->type_id, j->n, j->posted, j->refused, executed,
                                 !j->ran ? "not_run" : j->unexplained ? "unexplained" : j->refused ? (j->posted ? "partly_refused" : "refused") : "posted",
                                 j->reason, j->ran_us / 1000.0, j->requeues, j->forbid, j->post_us, rb_us >= 0 ? rb_us / 1000.0 : -1.0,
                                 (mono_us() - t0) / 1000.0, j->player_tag);
    for (int i = 0; i < j->n && at < n; i++)
        at += (size_t)snprintf(out + at, n - at, "%s{\"obj\":\"0x%llx\",\"result\":\"%s\"%s}", i ? "," : "", (unsigned long long)j->obj[i],
                               R_NAMES[j->res[i]], "");
    if (at < n) snprintf(out + at, n - at, "]");
    logj("\"event\":\"post2\",\"id\":%ld,%s", id, out);
    int ok = j->ran && !j->unexplained && j->refused == 0 && j->posted == j->n && (pending == 0);
    job2_release(j);
    return ok;
}

static void be_log(const char *members) { logj("%s", members); }

static void *ipc_thread(void *arg) {
    pthread_setname_np("hoi4-live-inproc-ipc");
    static const ipc_backend be = {be_hello, be_read, be_probe, be_post_speed, be_post2, killed, be_log};
    ipc_serve((const char *)arg, &be, &g_policy);
    return NULL;
}

static void *worker(void *arg) {
    (void)arg;
    pthread_setname_np("hoi4-live-inproc");
    const char *d = getenv("HOI4_INPROC_DELAY_S");
    unsigned delay = d ? (unsigned)atoi(d) : 20;
    sleep(delay); /* let the game finish starting before any work */
    if (killed()) { logj("\"event\":\"inert\",\"reason\":\"kill switch\""); return NULL; }

    caps_table *t = &g_t;
    long long t0 = now_ms();
    g_slide = _dyld_get_image_vmaddr_slide(0);
    if (!scan_image_open_process(&g_im, _dyld_get_image_header(0), g_slide)) {
        logj("\"event\":\"inert\",\"reason\":\"main image is not an x86_64 Mach-O the scanner can read\"");
        return NULL;
    }
    int resolved = caps_resolve(&g_im, t);
    long long t1 = now_ms();
    static char js[65536];
    logj("\"event\":\"capabilities\",\"resolved\":%d,\"of\":%d,\"scan_ms\":%lld,\"slide\":\"0x%llx\",\"table\":%s",
         resolved, CAP_COUNT, t1 - t0, (unsigned long long)g_slide, caps_json(&g_im, t, js, sizeof js));
    { /* per-build cache, read by `doctor`; the module rescans every start (the scan is cheap), so a stale cache is harmless */
        char p[1200], dir[1024];
        const char *c = getenv("HOI4_INPROC_CACHE");
        snprintf(dir, sizeof dir, "%s", c ? c : g_log_path);
        if (!c) { char *s = strrchr(dir, '/'); if (s) *s = 0; }
        snprintf(p, sizeof p, "%s/inproc-caps-%s.json", dir, g_im.uuid);
        int fd = open(p, O_WRONLY | O_CREAT | O_TRUNC, 0600);
        if (fd >= 0) { ssize_t w = write(fd, js, strlen(js)); (void)w; close(fd); }
    }
    if (t->cap[CAP_RESEARCH_FACTORY].ok) { /* cross-check: the dynamic linker finds the same factory by name */
        void *s = dlsym(RTLD_DEFAULT, "_ZN14CCommandHelperI19CSetResearchCommandE18CommandRegistrator6CreateEv");
        logj("\"event\":\"dlsym_check\",\"dlsym\":\"%p\",\"scan\":\"0x%llx\",\"match\":%s", s,
             (unsigned long long)(t->cap[CAP_RESEARCH_FACTORY].value + (uint64_t)g_slide),
             (uint64_t)(uintptr_t)s == t->cap[CAP_RESEARCH_FACTORY].value + (uint64_t)g_slide ? "true" : "false");
    }
    if (t->speed.ok) {
        void *s = dlsym(RTLD_DEFAULT, "_ZN14CCommandHelperI20CSetGameSpeedCommandE18CommandRegistrator6CreateEv");
        int m = (uint64_t)(uintptr_t)s == t->speed.factory + (uint64_t)g_slide;
        logj("\"event\":\"dlsym_check\",\"what\":\"speed_factory\",\"match\":%s", m ? "true" : "false");
        if (!m) { t->speed.ok = 0; t->cap[CAP_SPEED_CMD].ok = 0; }
    }
    if (!t->cap[CAP_GAMESTATE_VTABLE].ok || !t->cap[CAP_IDLER_VTABLE].ok) {
        logj("\"event\":\"inert\",\"reason\":\"roots unresolved: reads off\"");
        return NULL;
    }
    /* STAGE 1 policy: posting needs the launcher's explicit opt-in AND every stage-1 capability on this build */
    const char *sock = g_sock_env[0] ? g_sock_env : NULL, *optin = g_post_env[0] ? g_post_env : NULL;
    g_policy.speed_max = t->speed.range_max; g_policy.min_interval_ms = 200; g_policy.max_per_minute = 60;
    if (!ipc_optin_valid(optin, g_policy.post_off_reason, sizeof g_policy.post_off_reason)) g_policy.post_enabled = 0;
    else if (!(t->cap[CAP_POST].ok && t->cap[CAP_QUEUE_ROOT].ok && t->cap[CAP_POST_LAYOUT].ok && t->cap[CAP_SPEED_CMD].ok &&
               t->cap[CAP_FORBID_TLV].ok && t->cap[CAP_GAMESTATE_VTABLE].ok && t->cap[CAP_IDLER_VTABLE].ok)) {
        g_policy.post_enabled = 0;
        snprintf(g_policy.post_off_reason, sizeof g_policy.post_off_reason, "a stage-1 capability did not resolve on this build");
    } else if (t->cap[CAP_QUEUE_ROOT].extra % 8 || t->cap[CAP_QUEUE_ROOT].extra > 0x800) {
        g_policy.post_enabled = 0;
        snprintf(g_policy.post_off_reason, sizeof g_policy.post_off_reason, "queue getter slot implausible");
    } else {
        g_policy.post_enabled = 1;
        snprintf(g_policy.post_off_reason, sizeof g_policy.post_off_reason, "on: %s", optin);
    }
    /* stage 1 (speed) and the stage-2 families: each needs its name in the launcher's opt-in and its scans */
    g_policy.fam_on[0] = g_policy.post_enabled && ipc_optin_has(optin, "set_game_speed");
    g_policy.stage2_jobs_per_minute = 600; g_policy.stage2_cmds_per_minute = 2400;
    for (int i = 1; i < IPC_ALLOWED_COUNT && i < IPC_MAX_FAMILIES; i++) {
        int named = ipc_optin_has(optin, IPC_ALLOWED[i]), resolved = stage2_family_on(&t->st2, IPC_ALLOWED[i]);
        g_policy.fam_on[i] = g_policy.post_enabled && named && resolved && t->cap[CAP_STAGE2_COMMON].ok;
        snprintf(g_policy.fam_reason[i], sizeof g_policy.fam_reason[i], "%s", g_policy.fam_on[i] ? "on" : !g_policy.post_enabled ? "posting off" :
                 !named ? "not named in the launcher's opt-in" : "a command type of the family did not resolve on this build");
    }
    logj("\"event\":\"post_policy\",\"enabled\":%s,\"reason\":\"%s\",\"speed\":%s,\"production\":\"%s\",\"air\":\"%s\",\"min_interval_ms\":%ld,\"max_per_minute\":%d",
         g_policy.post_enabled ? "true" : "false", g_policy.post_off_reason, g_policy.fam_on[0] ? "true" : "false",
         g_policy.fam_reason[1], g_policy.fam_reason[2], g_policy.min_interval_ms, g_policy.max_per_minute);

    uint64_t *cand; unsigned *refs;
    size_t nc = scan_global_candidates(&g_im, 200, &cand, &refs);
    g_vt_gs = t->cap[CAP_GAMESTATE_VTABLE].value + (uint64_t)g_slide;
    g_vt_idler = t->cap[CAP_IDLER_VTABLE].value + (uint64_t)g_slide;
    int loaded = 0, polls = 0, ipc_started = 0;
    static char sockp[1024];
    reads_t last; memset(&last, 0, sizeof last);
    long long last_log = 0;
    logj("\"event\":\"waiting\",\"global_candidates\":%zu", nc);
    for (;;) {
        sleep(2);
        polls++;
        if (killed()) { logj("\"event\":\"inert\",\"reason\":\"kill switch\""); break; }
        if (!g_gs_glob || !g_idler_glob) { /* roots by dynamic type: the global whose object's vtable is the RTTI-found one */
            uint64_t ggs = 0, gid = 0;
            for (size_t i = 0; i < nc; i++) {
                uint64_t p, vp;
                if (!sq(cand[i] + (uint64_t)g_slide, &p) || !plausible(p) || !sq(p, &vp)) continue;
                if (vp == g_vt_gs && !ggs) ggs = cand[i];
                if (vp == g_vt_idler && !gid) gid = cand[i];
            }
            if (ggs && gid) {
                g_gs_glob = ggs; g_idler_glob = gid;
                logj("\"event\":\"roots\",\"gamestate_global\":\"0x%llx\",\"idler_global\":\"0x%llx\","
                     "\"idler_is_queue_root\":%s,\"gamestate_is_execute_global\":%s,\"polls\":%d",
                     (unsigned long long)ggs, (unsigned long long)gid, gid == t->cap[CAP_QUEUE_ROOT].value ? "true" : "false",
                     ggs == t->speed.gs_global ? "true" : "false", polls);
                if (gid != t->cap[CAP_QUEUE_ROOT].value || ggs != t->speed.gs_global) {
                    g_policy.post_enabled = 0;
                    snprintf(g_policy.post_off_reason, sizeof g_policy.post_off_reason, "roots by dynamic type differ from the scanned globals");
                }
            }
            continue;
        }
        if (sock && *sock && !ipc_started && strlen(sock) < sizeof sockp) { /* the channel starts once a game is loaded */
            snprintf(sockp, sizeof sockp, "%s", sock);
            pthread_t th; pthread_attr_t a;
            pthread_attr_init(&a); pthread_attr_setdetachstate(&a, PTHREAD_CREATE_DETACHED);
            if (pthread_create(&th, &a, ipc_thread, sockp)) logj("\"event\":\"ipc_off\",\"reason\":\"no thread\"");
            pthread_attr_destroy(&a);
            ipc_started = 1;
        }
        reads_t r;
        if (!do_reads(&r)) { if (loaded == 1) { logj("\"event\":\"not_in_game\""); loaded = 0; } continue; }
        if (!g_t.fields && !loaded) logj("\"event\":\"reads_partial\",\"reason\":\"no field table for this build: speed only\"");
        long long now = now_ms();
        int changed = r.speed != last.speed || r.paused != last.paused || r.tag != last.tag;
        long long every = polls < 300 ? 10000 : 60000; /* every 10 s for the first 10 minutes, then every minute */
        if (loaded != 1 || changed || now - last_log >= every) {
            char b[400]; reads_json(&r, b, sizeof b);
            logj("\"event\":\"%s\",%s", loaded != 1 ? "game_loaded" : "read", b);
            last_log = now;
        }
        loaded = 1;
        last = r;
    }
    free(cand); free(refs);
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
    g_log = open(g_log_path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
    if (g_log < 0) return;
    /* stage-1 switches: copied now (single-threaded) and removed so the game and its children never see them */
    const char *sp = getenv("HOI4_INPROC_SOCK"), *pp = getenv("HOI4_INPROC_POST");
    if (sp && strlen(sp) < sizeof g_sock_env) snprintf(g_sock_env, sizeof g_sock_env, "%s", sp);
    if (pp && strlen(pp) < sizeof g_post_env) snprintf(g_post_env, sizeof g_post_env, "%s", pp);
    unsetenv("HOI4_INPROC_SOCK"); unsetenv("HOI4_INPROC_POST");
    logj("\"event\":\"loaded\",\"module\":\"%s\",\"pid\":%d,\"exe\":\"%s\",\"post_opt_in\":%s,\"channel\":%s", MODULE_VERSION,
         (int)getpid(), exe, g_post_env[0] ? "true" : "false", g_sock_env[0] ? "true" : "false");
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
