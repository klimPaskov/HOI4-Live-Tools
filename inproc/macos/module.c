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
#include "../core/mio.h"
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
/* engineer 73: battle plans */
typedef struct { int32_t *data; int32_t capacity, size; } pdx_iarray; /* CPdxArray<int> {data, capacity, size} */
typedef void (*ctor_order_path)(void *, void *, const pdx_iarray *, int);
typedef void (*ctor_new_front)(void *, void *, const pdx_iarray *, void *, const pdx_array *, int, int, int, void *);
typedef void *(*order_lookup_fn)(void *, int32_t);
/* engineer 75: army structure */
typedef void (*ctor_divs_theatre)(void *, const pdx_array *, void *, void *);
typedef void (*ctor_leader)(void *, void *, void *, int);
typedef void (*ctor_group_name)(void *, void *, const void *);
typedef void (*ctor_event_option)(void *, int32_t, void *, const int32_t *, const void *, int32_t); /* engineer 76 */
typedef void (*ctor_area_state)(void *, void *, int32_t, int32_t, int32_t); /* engineer 79: (cmd, group, order id, state, settings) */
typedef void (*ctor_research)(void *, const int32_t *, void *, int32_t, int, int, int); /* engineer 79: (cmd, &tag, template, slot, b34, b35, b36) */
typedef void (*ctor_idea)(void *, const int32_t *, void *);           /* engineer 79: AddIdea (cmd, &tag, idea) */
typedef void (*ctor_idea2)(void *, const int32_t *, void *, void *);  /* engineer 79: ReplaceIdea (cmd, &tag, old, new) */
typedef void (*ctor_construction)(void *, int, const int32_t *, const void *, int32_t); /* engineer 82: (cmd, mode, &tag, &reference, count) */
typedef unsigned char (*option_test_fn)(void *, const void *, unsigned char);
typedef void (*ctor_groups_obj)(void *, const pdx_array *, void *);
typedef void (*ctor_groups_obj_i32)(void *, const pdx_array *, void *, int32_t);
/* engineer 74: a CPdxArray allocator's allocate slot: alloc(allocator, bytes, align) */
typedef void *(*alloc_fn)(void *, uint64_t, uint64_t);
#define J2_PROVS 1024
#define J2_DIVS 512

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
    /* orders (engineer 73): the province list, the order named by id (vals[0]), the divisions a new front gets, the
     * drawing group, and the before-state the read-back compares */
    int32_t provs[J2_PROVS]; int nprov;
    uint64_t order, drawer; int32_t order_id;
    uint64_t divs[J2_DIVS]; int ndiv;
    int32_t before_children; uint8_t before_exec;
    /* orders2 (engineer 74): the divisions named by the request (CUnit pointers, checked) and their handles, the
     * front's raw handle (new_root), the before-state of the read-back, a spearhead's blitz byte */
    uint64_t divh[J2_DIVS]; int ndivh;
    uint64_t hx, theatre; int32_t before_top, before_fallback; int blitz;
    /* units (engineer 75): each division's army before the post (the read-back compares), a new name (std::string
     * short form: at most 22 bytes), divisions not yet in an army are legal for a new army / a transfer */
    uint64_t grp_before[J2_DIVS]; char name[24]; int nname; int allow_free;
    /* events (engineer 76): the window's pending id, event, its handle, the country, the option chosen (shown place ->
     * the event's own option index) */
    int32_t ev_id, ev_country, ev_option; uint64_t ev_event, ev_handle;
    /* holds (engineer 79): the area order's settings mask the click carries, the group's area-order ids before a new
     * order is created (the read-back finds the one new id), the new order's id */
    int32_t hold_setting, hold_before[64], new_order_id; int hold_nbefore;
    /* construction (engineer 82): the building reference the click builds (location id, building id) and the kind of
     * location the request names (0 a state, 1 a province) */
    int32_t bld_loc, bld_id, bld_kind;
    uint64_t deploy; /* training (engineer 82): the player's deployment status the Train check reads */
    /* mio (engineer 84): the player's organisation manager, the token the command carries (a trait, a policy or the
     * empty token = detach), the before-state the read-back compares, the organisation's handle (line) */
    uint64_t mio_mgr, mio_h2; int32_t mio_tok, mio_before;
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
    if (ct->spec->auth_kind == AUTH_EVENT) { /* engineer 76: an open event window, by its exact class */
        uint64_t vt = 0;
        *handle = 0;
        return g_t.st2.events.ok && plausible(obj) && sq(obj, &vt) && vt == g_t.st2.events.vt_window + (uint64_t)g_slide;
    }
    if (ct->spec->auth_kind == AUTH_TAG_PTR) { /* engineer 75: a raw pointer of exactly the class the spec names */
        uint64_t vt = 0;
        *handle = 0;
        return ct->vt_obj && plausible(obj) && sq(obj, &vt) && vt == ct->vt_obj + (uint64_t)g_slide;
    }
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

/* engineer 73: the order a battle-plan command names, found with the game's OWN lookup (the function every order
 * command's "can do" check calls: a pure read over the group's three lists), on the main thread after the group passed
 * its resolver and owner checks. Returns NULL when ready, else the reason (nothing is posted). */
typedef void *(*div_get_fn)(void *);
static const char *orders_prep(job2_t *j) {
    const orders_facts *of = &g_t.st2.orders;
    const char *op = j->ct->spec->op;
    uint64_t grp = j->obj[0], vt = of->vt_order + (uint64_t)g_slide, v = 0, own = 0;
    static char why[160];
    if (!strcmp(op, "orders.execute") && j->order_id == 0) return NULL; /* 0 = the whole plan (the plan bar's button) */
    int new_front = !strcmp(op, "orders.new_front") || !strcmp(op, "orders2.spearhead");
    /* orders2 ops without an order: nothing to find here (new_root, unassign, fallback) */
    if (!strcmp(op, "orders2.new_root") || !strcmp(op, "orders2.unassign") || !strcmp(op, "orders2.fallback")) return NULL;
    uint64_t lk = of->lookup_fn + (uint64_t)g_slide;
    if (!code_ptr(lk)) return "the order lookup is not game code";
    uint64_t o = (uint64_t)(uintptr_t)((order_lookup_fn)(uintptr_t)lk)((void *)(uintptr_t)grp, j->order_id);
    if (!o || !sq(o, &v) || v != vt) { snprintf(why, sizeof why, "order %d is not in this group's plan", j->order_id); return why; }
    if (!sq(o + of->owner_off, &own) || own != grp) {
        snprintf(why, sizeof why, "order %d belongs to another orders group (name that group)", j->order_id); return why;
    }
    j->order = o;
    if (!strcmp(op, "orders.execute") && !sread(o + of->exec_off, &j->before_exec, 1)) return "executing byte unreadable";
    if (new_front) {
        int n = orders_free_divisions(of, sread, o, j->divs, J2_DIVS);
        if (n < 0) return "the parent's divisions are unreadable";
        /* engineer 89 (S4 O3, 27 Nov): the list holds each division's membership base, not the division; the front
         * tool hands NewFront the divisions, so each entry goes through its own getter (the UI's call), the result
         * must be the entry less the base offset, and a division whose alive byte is 0 is left out as the UI does */
        if (!of->div_get_slot || !of->div_sub_off) return "the front tool's division getter was not found on this build";
        int k = 0;
        for (int i = 0; i < n; i++) {
            uint64_t e = j->divs[i], evt = 0, fn = 0, d = 0; uint8_t alive = 0;
            if (!plausible(e) || !sq(e, &evt) || !sq(evt + of->div_get_slot, &fn) || !code_ptr(fn))
                { snprintf(why, sizeof why, "division entry %d (0x%llx) has no getter", i, (unsigned long long)e); return why; }
            d = (uint64_t)(uintptr_t)((div_get_fn)(uintptr_t)fn)((void *)(uintptr_t)e);
            if (d != e - of->div_sub_off || !sread(d + of->div_alive_off, &alive, 1))
                { snprintf(why, sizeof why, "division entry %d is not division + 0x%x", i, of->div_sub_off); return why; }
            if (alive) j->divs[k++] = d;
        }
        j->ndiv = k;
        if (!sread(o + of->child_cnt[0], &j->before_children, 4)) return "the parent's children are unreadable";
        j->drawer = grp;
        if (j->obj2) { /* the drawing group (the UI's selected army or army group): a live own COrdersGroup */
            const cmd_type *dt = stage2_find(&g_t.st2, "orders.delete");
            uint64_t h = 0, owner_f = dt ? dt->owner_fn + (uint64_t)g_slide : 0;
            if (!dt || !dt->ok || !roundtrip(dt, j->obj2, &h) || !code_ptr(owner_f) ||
                ((owner_fn2)(uintptr_t)owner_f)((void *)(uintptr_t)j->obj2) != j->player_tag)
                return "obj2 (the drawing group) is not a live own army or army group";
            j->drawer = j->obj2;
        }
    }
    return NULL;
}

/* engineer 74: every division a request names, checked the way COrderAssignCommand's own check reads it: the handle at
 * the unit's reference base resolves (the game's generic lookup) to it and the C++ cast to CUnit accepts it; its army
 * (the unit's group field) must be a live own COrdersGroup. Fills j->divh with the handles. NULL when all pass. */
static const char *orders2_divisions(job2_t *j) {
    const stage2_common *c = &g_t.st2.common;
    const cmd_type *dt = stage2_find(&g_t.st2, "orders.delete");
    static char why[160];
    if (!g_dyncast) g_dyncast = (dyncast_fn)dlsym(RTLD_DEFAULT, "__dynamic_cast");
    uint64_t lk = dt ? dt->lookup_fn + (uint64_t)g_slide : 0, owner_f = dt ? dt->owner_fn + (uint64_t)g_slide : 0;
    if (!dt || !dt->ok || !g_dyncast || !code_ptr(lk) || !code_ptr(owner_f) || !c->ti_unit) return "division facts unresolved";
    for (int i = 0; i < j->ndiv; i++) {
        uint64_t d = j->divs[i], h = 0, grp = 0, hg = 0;
        void *ref = NULL;
        if (!plausible(d) || !sq(d + c->unit_hint + c->cref_handle_off, &h) || !h ||
            (ref = ((resolve_fn2)(uintptr_t)lk)(&h)) == NULL || (uint64_t)(uintptr_t)ref != d + c->unit_hint ||
            (uint64_t)(uintptr_t)g_dyncast(ref, (void *)(uintptr_t)(c->ti_reference + (uint64_t)g_slide),
                                           (void *)(uintptr_t)(c->ti_unit + (uint64_t)g_slide), c->unit_hint) != d) {
            snprintf(why, sizeof why, "division %d (0x%llx) is not a live division", i, (unsigned long long)d); return why;
        }
        if (!sq(d + c->unit_group_off, &grp)) { snprintf(why, sizeof why, "division %d is unreadable", i); return why; }
        j->grp_before[i] = grp;
        /* units: a division in no army yet (its group 0) is legal: the game's own filter keeps only the country's */
        if (!(grp == 0 && j->allow_free) && (!roundtrip(dt, grp, &hg) ||
            ((owner_fn2)(uintptr_t)owner_f)((void *)(uintptr_t)grp) != j->player_tag)) {
            snprintf(why, sizeof why, "division %d is not in an own army", i); return why;
        }
        for (int k = 0; k < i; k++) if (j->divh[k] == h) { snprintf(why, sizeof why, "division %d listed twice", i); return why; }
        j->divh[i] = h;
    }
    j->ndivh = j->ndiv;
    return NULL;
}

/* engineer 74: new_root's front: an exact CFront of the group's own theatre with the named section (provinces > 0); the
 * front's own handle (at its reference base) must resolve to it. */
static const char *orders2_front(job2_t *j) {
    const fronts_facts *ff = &g_t.st2.fronts;
    const stage2_common *c = &g_t.st2.common;
    const cmd_type *dt = stage2_find(&g_t.st2, "orders.delete");
    uint64_t v = 0, h = 0, lk = dt ? dt->lookup_fn + (uint64_t)g_slide : 0;
    if (!ff->ok || !dt || !code_ptr(lk)) return "front facts unresolved";
    if (!plausible(j->obj2) || !sq(j->obj2, &v) || v != ff->vt_front + (uint64_t)g_slide) return "obj2 is not a live CFront";
    j->theatre = fronts_theatre_of(ff, sread, ff->vt_group + (uint64_t)g_slide, ff->vt_theatre + (uint64_t)g_slide, j->obj[0]);
    if (!j->theatre) return "the group has no theatre (no front to bind a root to)";
    if (!fronts_has_section(ff, sread, ff->vt_front + (uint64_t)g_slide, ff->vt_section + (uint64_t)g_slide, j->theatre, j->obj2,
                            (int32_t)j->val[0]))
        return "the front is not in the group's theatre or has no such section";
    if (!sq(j->obj2 + c->cref_handle_off, &h) || !h || (uint64_t)(uintptr_t)((resolve_fn2)(uintptr_t)lk)(&h) != j->obj2)
        return "the front's handle does not resolve to it";
    j->hx = h;
    return NULL;
}

/* engineer 75: the theatre a new army or army group is made in, the way the UI finds it. new_army: the first named
 * division's theatre (the getter the new-army card calls on the first selected division), which must be an exact
 * CTheatre; army_group_new: the armies (live own orders groups, checked like every order command's group) must share one
 * theatre (their plain parents up to the CTheatre), and their handles become the command's list. The theatre replaces
 * obj[0], so the type's own resolver and owner checks then run on it. NULL when ready. */
static const char *units_theatre(job2_t *j) {
    const fronts_facts *ff = &g_t.st2.fronts;
    const stage2_common *c = &g_t.st2.common;
    const cmd_type *dt = stage2_find(&g_t.st2, "orders.delete");
    uint64_t vth = ff->vt_theatre + (uint64_t)g_slide, v = 0, th = 0;
    static char why[160];
    if (!ff->ok || !dt || !dt->ok) return "front/order facts unresolved";
    if (!strcmp(j->ct->spec->op, "units.new_army")) {
        j->allow_free = 1;
        const char *w = orders2_divisions(j); /* every division a live CUnit (in an own army or in none) */
        if (w) return w;
        if (!sq(j->divs[0] + c->unit_theatre_off, &th) || !plausible(th) || !sq(th, &v) || v != vth)
            return "the first division has no theatre (an exact CTheatre)";
    } else {
        uint64_t owner_f = dt->owner_fn + (uint64_t)g_slide, h = 0;
        if (!code_ptr(owner_f)) return "owner getter not game code";
        for (int i = 0; i < j->ndiv; i++) {
            if (!roundtrip(dt, j->divs[i], &h) || ((owner_fn2)(uintptr_t)owner_f)((void *)(uintptr_t)j->divs[i]) != j->player_tag) {
                snprintf(why, sizeof why, "army %d (0x%llx) is not a live own army", i, (unsigned long long)j->divs[i]); return why;
            }
            uint64_t t2 = fronts_theatre_of(ff, sread, ff->vt_group + (uint64_t)g_slide, vth, j->divs[i]);
            if (!t2) { snprintf(why, sizeof why, "army %d has no theatre", i); return why; }
            if (i && t2 != th) return "the armies stand in different theatres (one army group per theatre)";
            th = t2; j->divh[i] = h;
        }
        j->ndivh = j->ndiv;
    }
    j->theatre = th; j->obj[0] = th; j->n = 1;
    return NULL;
}

/* engineer 75: the order the UI's transfer (a right click on an army's card with divisions selected, 1.20 0x100d28480)
 * names: walk the army and its parent groups for the first with top orders; take its first top order; unless that
 * order's kind is 3, descend through first children while there are any; its id. 0 when no group has a plan. */
static int32_t units_transfer_order(uint64_t grp) {
    const orders_facts *of = &g_t.st2.orders;
    uint32_t par = g_t.st2.common.group_parent_ptr_off;
    for (int depth = 0; grp && depth < 8; depth++) {
        int32_t cnt = 0; uint64_t data = 0, o = 0;
        if (!sread(grp + of->list_cnt[0], &cnt, 4)) return -1;
        if (cnt > 0) {
            int32_t kind = 0, cc = 0, id = 0;
            if (!sq(grp + of->list_off[0], &data) || !sq(data, &o) || !plausible(o) || !sread(o + of->kind_off, &kind, 4)) return -1;
            for (int k = 0; kind != 3 && k < 64; k++) {
                uint64_t cd = 0;
                if (!sread(o + of->child_cnt[0], &cc, 4)) return -1;
                if (cc <= 0) break;
                if (!sq(o + of->child_off[0], &cd) || !sq(cd, &o) || !plausible(o)) return -1;
            }
            return sread(o + of->id_off, &id, 4) ? id : -1;
        }
        if (!sq(grp + par, &grp)) return -1;
    }
    return 0;
}

/* engineer 75: what the UI's constructor built must be exactly what was asked (units). Sets *filtered when the game's
 * own selection filter (the new-army constructor keeps only divisions it may move) dropped some: a refusal, not a fault. */
static const char *units_payload(job2_t *j, char *cmd, uint64_t h, uint64_t h2, uint64_t nullref, int *filtered) {
    const cmd_type *ct = j->ct;
    const cmd_field *f = ct->spec->f;
    uint64_t a = (uint64_t)(uintptr_t)cmd, d = 0, x = 0, y = 0, z = 0;
    int32_t n = -1, iv = 0; uint8_t b = 9;
    const uint64_t *want = NULL; int nwant = 0;
    switch (ct->spec->ctor) {
    case CTOR_DIVS_THEATRE:
        if (!sq(a + f[1].off, &x) || x != h || !sq(a + 0x48, &y) || y != nullref || !sq(a + 0x50, &z) || z != nullref ||
            !sread(a + 0x24, &b, 1) || b != 0)
            return "new army: theatre / theater group / army group / flag not as asked";
        want = j->divh; nwant = j->ndivh; break;
    case CTOR_LEADER:
        if (!sq(a + f[0].off, &x) || x != (j->obj2 ? h2 : nullref) || !sq(a + f[1].off, &y) || y != h || !sread(a + f[2].off, &b, 1) || b != 0)
            return "leader: leader / group / flag not as asked";
        return NULL;
    case CTOR_GROUP_NAME: {
        uint8_t got[24];
        if (!sq(a + f[0].off, &x) || x != h || !sread(a + f[1].off, got, 24) || got[0] != (uint8_t)(2 * j->nname) ||
            memcmp(got + 1, j->name, (size_t)j->nname) || got[1 + j->nname] != 0)
            return "rename: group / name not as asked";
        return NULL;
    }
    case CTOR_GROUPS_OBJ:
        if (!sq(a + f[1].off, &x) || x != h) return "army group: theatre not as asked";
        want = j->divh; nwant = j->ndivh; break;
    case CTOR_GROUPS_OBJ_I32:
        if (!sq(a + f[1].off, &x) || x != h2 || !sread(a + f[2].off, &iv, 4) || iv != -1) return "into army group: target / index not as asked";
        /* fall through: the list = the armies */
    case CTOR_GROUPS:
        want = NULL; nwant = j->n; break;
    default: return "unknown units constructor";
    }
    if (!sq(a + f[0].off, &d) || !sread(a + f[0].count_off, &n, 4) || n < 0 || n > nwant) return "list not as asked";
    if (n < nwant) { *filtered = nwant - n; return NULL; }
    for (int e = 0; e < n; e++) {
        uint64_t got = 0, w = 0;
        if (want) w = want[e]; else { uint64_t hh = 0; if (!sq(j->obj[e] + ct->target_hint + g_t.st2.common.cref_handle_off, &hh)) return "list unreadable"; w = hh; }
        if (!sq(d + 8 * (uint64_t)e, &got) || got != w) return "list not in the asked order";
    }
    return NULL;
}

/* engineer 74: fill one CPdxArray field of the fresh command through the allocator the factory put there */
static int fill_array(char *cmd, const cmd_field *f, const void *src, int n, int esize) {
    uint64_t data = 1, al = 0, avt = 0, afn = 0; int32_t cap = -1, size = -1;
    uint32_t aoff = f->count_off + 4; /* {data, capacity, size, allocator} */
    if (f->count_off != f->off + 0xc || !sq((uint64_t)(uintptr_t)cmd + f->off, &data) || data != 0 ||
        !sread((uint64_t)(uintptr_t)cmd + f->off + 8, &cap, 4) || cap != 0 || !sread((uint64_t)(uintptr_t)cmd + f->count_off, &size, 4) ||
        size != 0 || !sq((uint64_t)(uintptr_t)cmd + aoff, &al) || !plausible(al) || !sq(al, &avt) || !plausible(avt) ||
        !sq(avt + g_t.st2.common.alloc_slot, &afn) || !code_ptr(afn))
        return 0;
    if (n == 0) return 1;
    void *mem = ((alloc_fn)(uintptr_t)afn)((void *)(uintptr_t)al, (uint64_t)n * (uint64_t)esize, 8);
    if (!mem) return 0;
    memcpy(mem, src, (size_t)n * (size_t)esize);
    *(void **)(cmd + f->off) = mem;
    *(int32_t *)(cmd + f->off + 8) = n;
    *(int32_t *)(cmd + f->count_off) = n;
    return 1;
}

/* engineer 76: an event answer, everything taken from the open window as its own option click takes it. The window
 * must accept a click now (its gate byte), name the player's country, hold an event that resolves (generic lookup, the
 * auth slot's cast to CEvent) and has a pending record of the window's id in the game state (the record holds that
 * event). The option: vals[0] = its place among the shown options, vals[1] = how many the window shows; the module
 * walks the event's options with the game's own option test (the window's own per-frame test) and refuses (GUI path)
 * when its count differs from the shown count. Returns NULL when ready; *unavailable = 1 when the game's test says no. */
static const char *events_prep(job2_t *j, uint64_t gs, int *unavailable) {
    const events_facts *e = &g_t.st2.events;
    uint64_t w = j->obj[0], ev = 0, h = 0, lk = j->ct->lookup_fn + (uint64_t)g_slide, arr = 0;
    uint8_t open = 0, flag = 0; int32_t cnt = 0, nopt = 0;
    *unavailable = 0;
    if (!sread(w + e->win_open_off, &open, 1) || !open) return "the event window does not accept a click now";
    if (!sread(w + e->win_id_off, &j->ev_id, 4) || !sq(w + e->win_event_off, &ev) || !plausible(ev) ||
        !sread(w + e->win_country_off, &j->ev_country, 4) || !sread(w + e->win_flag_off, &flag, 1)) return "the window is unreadable";
    if (j->ev_country != j->player_tag) {
        /* s6v (24 Nov): a news event (news.184) was refused here: the values go into the reason for the next look */
        static char why[128];
        snprintf(why, sizeof why, "the event is not the player's country's (window +0x%x: %d, player %d, id %d)",
                 (unsigned)e->win_country_off, j->ev_country, j->player_tag, j->ev_id);
        return why;
    }
    if (!g_dyncast) g_dyncast = (dyncast_fn)dlsym(RTLD_DEFAULT, "__dynamic_cast");
    if (!g_dyncast || !code_ptr(lk) || !sq(ev + g_t.st2.common.cref_handle_off, &h) || !h || (uint64_t)(uintptr_t)((resolve_fn2)(uintptr_t)lk)(&h) != ev ||
        (uint64_t)(uintptr_t)g_dyncast((void *)(uintptr_t)ev, (void *)(uintptr_t)(e->ti_reference + (uint64_t)g_slide),
                                       (void *)(uintptr_t)(e->ti_event + (uint64_t)g_slide), -1) != ev)
        return "the window's event is not a live CEvent";
    if (!sread(gs + e->pend_cnt_off, &cnt, 4) || cnt < 0 || cnt > 4096 || !sq(gs + e->pend_arr_off, &arr) || (cnt && !plausible(arr)))
        return "the pending events are unreadable";
    int found = 0;
    for (int32_t i = 0; i < cnt && !found; i++) {
        int32_t id = 0; uint64_t pe = 0, rec = arr + (uint64_t)i * e->pend_size;
        if (sread(rec, &id, 4) && id == j->ev_id && sq(rec + e->pend_event_off, &pe) && pe == ev) found = 1;
    }
    if (!found) return "no pending record of this event (already answered?)";
    if (!sread(ev + e->ev_nopts_off, &nopt, 4) || nopt < 1 || nopt > 64 || !sq(ev + e->ev_opts_off, &arr) || !plausible(arr))
        return "the event's options are unreadable";
    uint64_t tf = e->avail_fn + (uint64_t)g_slide;
    if (!code_ptr(tf)) return "the option test is not game code";
    int shown = 0, pick = -1;
    for (int32_t i = 0; i < nopt; i++) {
        uint64_t opt = 0; int32_t idx = -1;
        if (!sq(arr + 8 * (uint64_t)i, &opt) || !plausible(opt) || !sread(opt + e->opt_index_off, &idx, 4) || idx != i)
            return "an option's own index differs from its place";
        if (!((option_test_fn)(uintptr_t)tf)((void *)(uintptr_t)opt, (const void *)(uintptr_t)(w + e->win_scope_off), flag)) continue;
        if (shown == (int)j->val[0]) pick = i;
        shown++;
    }
    if (shown != (int)j->val[1]) { snprintf(j->reason, sizeof j->reason, "the game's option test passes %d options, the window shows %d", shown, (int)j->val[1]); return j->reason; }
    if (pick < 0) { *unavailable = 1; return "the option is not available"; }
    j->ev_option = pick; j->ev_event = ev; j->ev_handle = h;
    return NULL;
}


/* engineer 78: a diplomatic action, as the diplomacy view sends it. j->val[0] = the receiving country's tag id. The
 * module builds the action with the action class's own constructor (sender = the player, receiver = the target, the
 * game date; declare war: one of the player's own war goals against the target, as the declare-war dialog lists them),
 * hands it to the game's own command maker, asks the command's own "can do" (the action's legality virtual) and posts.
 * Only the module's fresh objects are written; refused = the game's own destructor frees both. */
typedef void *(*country_of_fn)(const int32_t *);
typedef void (*act_simple_fn)(void *, const int32_t *, const int32_t *, const void *);
typedef void (*act_war_fn)(void *, const int32_t *, const int32_t *, const void *, int, const void *);
typedef void **(*make_cmd_fn)(void **, void **);
typedef void *(*new_fn)(size_t);
static const char *diplo_build(job2_t *j, const cmd_type *ct, uint64_t gs, uint64_t q, uint64_t post) {
    const diplo_facts *d = &g_t.st2.diplo;
    const stage2_common *c = &g_t.st2.common;
    uint64_t sl = (uint64_t)g_slide;
    int32_t target = (int32_t)j->val[0], date = 0;
    static new_fn nw;
    if (!nw) nw = (new_fn)dlsym(RTLD_DEFAULT, "_Znwm");
    if (!nw || !code_ptr(d->country_of + sl) || !code_ptr(d->make_cmd + sl) || !code_ptr(ct->act_ctor + sl) || !code_ptr(post)) { j->unexplained = 1; return "diplomacy facts are not game code"; }
    if (target == j->player_tag) { j->res[0] = R_BAD_OBJ; return "the target is the player's own country"; }
    void *tc = ((country_of_fn)(uintptr_t)(d->country_of + sl))(&target);
    if (!tc) { j->res[0] = R_BAD_OBJ; return "the target tag is not a live country"; }
    if (!sread(gs + d->date_off, &date, 4) || date <= 0) { j->unexplained = 1; return "the game date is unreadable"; }
    uint64_t goal = 0;
    if (ct->spec->ctor == CTOR_DIPLO_WAR) { /* the player's own war goals whose target is the receiver: the first one */
        void *pc = ((country_of_fn)(uintptr_t)(d->country_of + sl))(&j->player_tag);
        uint64_t holder = 0, data = 0; int32_t cnt = -1;
        if (!pc || !sq((uint64_t)(uintptr_t)pc + d->wg_holder_off, &holder) || !plausible(holder) ||
            !sread(holder + d->wg_count_off, &cnt, 4) || cnt < 0 || cnt > 4096 || (cnt && (!sq(holder + d->wg_list_off, &data) || !plausible(data)))) {
            j->unexplained = 1; return "the player's war goals are unreadable";
        }
        for (int32_t k = 0; k < cnt && !goal; k++) {
            uint64_t g = 0, h = 0; int32_t t = 0;
            if (!sq(data + 8 * (uint64_t)k, &g) || !plausible(g) || !sread(g + d->wg_target_off, &t, 4) || t != target) continue;
            uint64_t lk = ct->lookup_fn + sl;
            if (!sq(g + c->cref_handle_off, &h) || !h || !code_ptr(lk) || (uint64_t)(uintptr_t)((resolve_fn2)(uintptr_t)lk)(&h) != g) continue;
            goal = g; j->hx = h;
        }
        if (!goal) { j->res[0] = R_INVALID; j->refused++; return "no war goal of the player's against the target (justify one first)"; }
    }
    /* the date object the constructors read (+8), laid out as they build their own: two vtables of the CDate group */
    uint64_t dobj[3] = {d->date_vt + sl + 0x10, (uint64_t)(uint32_t)date, d->date_vt + sl + 0x60};
    char *act = (char *)nw(ct->act_size);
    if (!act) { j->unexplained = 1; return "allocation failed"; }
    memset(act, 0, ct->act_size);
    if (ct->spec->ctor == CTOR_DIPLO_SIMPLE) ((act_simple_fn)(uintptr_t)(ct->act_ctor + sl))(act, &j->player_tag, &target, dobj);
    else ((act_war_fn)(uintptr_t)(ct->act_ctor + sl))(act, &j->player_tag, &target, (const void *)(uintptr_t)goal, 0, dobj);
    /* what the game built must be exactly what was asked */
    uint64_t av = 0, ah = 0; int32_t at = -1, as = -1, ar = -1;
    int ok = sq((uint64_t)(uintptr_t)act, &av) && av == ct->vt_obj + sl && sread((uint64_t)(uintptr_t)act + 8, &at, 4) &&
             at == (int32_t)ct->act_type && sread((uint64_t)(uintptr_t)act + ct->act_sender_off, &as, 4) && as == j->player_tag &&
             sread((uint64_t)(uintptr_t)act + ct->act_sender_off + 8, &ar, 4) && ar == target;
    if (ok && ct->spec->ctor == CTOR_DIPLO_WAR) {
        uint8_t fl = 9;
        ok = sq((uint64_t)(uintptr_t)act + ct->act_goal_off, &ah) && ah == j->hx && sread((uint64_t)(uintptr_t)act + ct->act_flag_off, &fl, 1) && fl == 0;
    }
    if (!ok) { void **vt0 = *(void ***)act; ((delete_fn2)vt0[1])(act); j->res[0] = R_UNEXPECTED; j->unexplained = 1; return "the action constructor built an unexpected object"; }
    void *up = act, *cmd = NULL;
    ((make_cmd_fn)(uintptr_t)(d->make_cmd + sl))(&cmd, &up); /* moves the action into a fresh command, stamps the date */
    if (!cmd) { if (up) { void **vt0 = *(void ***)up; ((delete_fn2)vt0[1])(up); } j->res[0] = R_UNEXPECTED; j->unexplained = 1; return "the command maker built nothing"; }
    uint64_t cv = 0, ca = 0; int32_t ad = 0;
    if (!sq((uint64_t)(uintptr_t)cmd, &cv) || cv != ct->vtable + sl || !sq((uint64_t)(uintptr_t)cmd + ct->spec->auth_off, &ca) ||
        ca != (uint64_t)(uintptr_t)act || !sread((uint64_t)(uintptr_t)act + d->act_date_off, &ad, 4) || ad != date) {
        j->res[0] = R_UNEXPECTED; j->unexplained = 1; return "the command maker built an unexpected command"; /* not posted; leaked rather than freed blind */
    }
    void **vt = *(void ***)cmd;
    if (!((valid_fn2)vt[c->valid_slot / 8])(cmd, 0)) { ((delete_fn2)vt[c->delete_slot / 8])(cmd); j->res[0] = R_INVALID; j->refused++; return NULL; }
    long long t0 = mono_us();
    unsigned char r = ((post_fn)(uintptr_t)post)((void *)(uintptr_t)q, cmd, 0);
    j->post_us = mono_us() - t0;
    if (!r) { j->res[0] = R_DROPPED; j->unexplained = 1; return "Post dropped a command every check passed"; }
    j->res[0] = R_POSTED; j->posted++;
    return NULL;
}

/* engineer 79: the ids of the group's area-defence orders (the kind the state click's execution creates), from the
 * group's own order lists (top, fallback, area); -1 on a read fault */
static int hold_area_ids(uint64_t grp, int32_t *ids, int max) {
    const orders_facts *of = &g_t.st2.orders;
    uint64_t vt = of->vt_order + (uint64_t)g_slide;
    int n = 0;
    for (int l = 0; l < 3; l++) {
        uint64_t d = 0; int32_t cnt = 0;
        if (!sq(grp + of->list_off[l], &d) || !sread(grp + of->list_cnt[l], &cnt, 4) || cnt < 0 || cnt > 512) return -1;
        for (int32_t e = 0; e < cnt; e++) {
            uint64_t o = 0, v = 0; int32_t k = 0, id = 0;
            if (!sq(d + 8 * (uint64_t)e, &o) || !sq(o, &v) || v != vt) return -1;
            if (!sread(o + of->kind_off, &k, 4) || !sread(o + of->id_off, &id, 4)) return -1;
            if ((uint32_t)k == g_t.st2.holds.kind_area && n < max) ids[n++] = id;
        }
    }
    return n;
}
/* 1 when the area order holds this state id (its own list, the one the click toggles), 0 when not, -1 on a fault */
static int hold_has_state(uint64_t order, int32_t state) {
    const holds_facts *hf = &g_t.st2.holds;
    uint64_t d = 0; int32_t cnt = 0;
    if (!sq(order + hf->area_off, &d) || !sread(order + hf->area_cnt, &cnt, 4) || cnt < 0 || cnt > 4096) return -1;
    for (int32_t e = 0; e < cnt; e++) { int32_t x = 0; if (!sread(d + 4 * (uint64_t)e, &x, 4)) return -1; if (x == state) return 1; }
    return 0;
}
/* engineer 79: an area-defence click. A NEW order (id 0): the tool's fresh settings (0) and the group's area orders
 * before (the read-back finds the new one). An existing order: the game's own order lookup finds it in THIS group, it is
 * of the area kind, and the state is not in it yet (the click toggles: a state already there would be REMOVED); the
 * click carries the order's own settings mask, as the tool does. NULL when ready. */
/* engineer 84: the organisations. The game's own functions, called on the main thread: tok2str (the name of a token),
 * can_unlock (the unlock command's own check), the policy item's click gate (cooldown, political power, allowed) */
typedef const void *(*tok2str_fn)(int32_t);
typedef unsigned char (*can_unlock_fn)(void *, const void *, const void *, int32_t, void *);
typedef unsigned char (*policy_gate_fn)(void *);
static int mio_rd_cb(uint64_t a, void *b, size_t n) { return sread(a, b, n); }
static uint64_t mio_name_cb(void *ctx, int32_t tok) {
    (void)ctx;
    uint64_t f = g_t.st2.mio.tok2str + (uint64_t)g_slide;
    return code_ptr(f) ? (uint64_t)(uintptr_t)((tok2str_fn)(uintptr_t)f)(tok) : 0;
}
static int mio_can_cb(void *ctx, uint64_t org, uint64_t trait) {
    (void)ctx;
    uint64_t f = g_t.st2.mio.can_unlock + (uint64_t)g_slide;
    return code_ptr(f) && ((can_unlock_fn)(uintptr_t)f)((void *)(uintptr_t)org, (const void *)(uintptr_t)trait,
                                                        (const void *)(uintptr_t)(org + g_t.st2.mio.unlocked_off), 1, NULL) != 0;
}
/* the policy window's item as its click reads it: the policy token and the organisation's handle; the gate reads
 * nothing else of the item (1.20 0x101ed8bf0: +0x70 and +0x74 only) */
static int mio_policy_cb(void *ctx, uint64_t org, uint64_t handle, int32_t policy) {
    (void)ctx; (void)org;
    const mio_facts *f = &g_t.st2.mio;
    uint64_t g = f->policy_gate + (uint64_t)g_slide;
    _Alignas(16) uint8_t item[0x200];
    if (!code_ptr(g) || f->gate_policy_off + 4 > sizeof item || f->gate_org_off + 8 > sizeof item) return 0;
    memset(item, 0, sizeof item);
    memcpy(item + f->gate_policy_off, &policy, 4);
    memcpy(item + f->gate_org_off, &handle, 8);
    return ((policy_gate_fn)(uintptr_t)g)(item) != 0;
}
/* the player's organisation manager: country_of(tag) -> +mgr_off */
static uint64_t mio_manager(int32_t tag) {
    const mio_facts *f = &g_t.st2.mio;
    uint64_t cf = f->country_of + (uint64_t)g_slide, mgr = 0;
    if (!f->ok || !code_ptr(cf)) return 0;
    void *pc = ((country_of_fn)(uintptr_t)cf)(&tag);
    if (!pc || !sq((uint64_t)(uintptr_t)pc + f->mgr_off, &mgr) || !plausible(mgr)) return 0;
    return mgr;
}
/* an organisation of the player's own list, of its exact class, its owner tag the player's */
static int mio_own_org(uint64_t mgr, uint64_t org, int32_t tag) {
    const mio_facts *f = &g_t.st2.mio;
    uint64_t vt = 0; int32_t t = 0;
    return plausible(org) && sq(org, &vt) && vt == f->vt_org + (uint64_t)g_slide && sread(org + f->tag_off, &t, 4) && t == tag &&
           mio_in_list(f, mio_rd_cb, mgr, org);
}
/* per object, after its round trip: what the organisation views allow before the click (the type's own "can do" runs
 * again before the post) */
static const char *mio_prep(job2_t *j, int i, int *invalid) {
    const mio_facts *f = &g_t.st2.mio;
    const char *op = j->ct->spec->op;
    static char why[200];
    *invalid = 0;
    if (!j->mio_mgr && !(j->mio_mgr = mio_manager(j->player_tag))) return "the player's organisation manager is unreadable";
    if (!strcmp(op, "mio.line")) { /* the line passed its resolver and owner; the organisation (if named) is own */
        if (j->obj2 && !mio_own_org(j->mio_mgr, j->obj2, j->player_tag)) return "obj2 is not one of the player's own organisations";
        if (j->obj2 && !sq(j->obj2 + g_t.st2.common.cref_handle_off, &j->mio_h2)) return "the organisation's handle is unreadable";
        if (!j->obj2 && !sq(g_t.st2.common.null_ref_global + (uint64_t)g_slide, &j->mio_h2)) return "the empty handle is unreadable";
        return NULL;
    }
    uint64_t org = j->obj[i], h = 0;
    if (!mio_own_org(j->mio_mgr, org, j->player_tag)) return "not one of the player's own organisations";
    if (!sq(org + g_t.st2.common.cref_handle_off, &h)) return "the organisation's handle is unreadable";
    int32_t tok = (int32_t)j->val[0];
    if (!strcmp(op, "mio.policy")) {
        int32_t cur = 0;
        if (!sread(org + f->policy_off, &cur, 4)) return "the attached policy is unreadable";
        /* the gate's cost routine reads the policy object unchecked: only a policy of the country's own list */
        if (!mio_policy(f, mio_rd_cb, j->mio_mgr, tok)) { snprintf(why, sizeof why, "policy %d is not one of the country's policies", tok); *invalid = 1; return why; }
        if (!mio_policy_cb(NULL, org, h, tok)) {
            *invalid = 1;
            return cur == tok ? "the policy window refuses to detach (its cooldown)" : "the policy window refuses (cooldown, political power or not allowed)";
        }
        j->mio_tok = cur == tok ? f->empty_tok : tok; /* the attached policy's item detaches (the UI sends the empty token) */
        j->val[0] = j->mio_tok;
        j->mio_before = cur;
        return NULL;
    }
    if (!mio_trait(f, mio_rd_cb, org, tok)) { snprintf(why, sizeof why, "trait %d is not in this organisation's tree", tok); *invalid = 1; return why; }
    j->mio_tok = tok;
    if (!strcmp(op, "mio.queue_trait")) {
        if (mio_queued(f, mio_rd_cb, org, tok) == 1) { *invalid = 1; return "the trait is queued already"; }
    } else {
        int u = mio_unlocked(f, mio_rd_cb, org, tok);
        if (u == 1) { *invalid = 1; return "the trait is unlocked already"; }
        if (u < 0) return "the unlocked traits are unreadable";
    }
    return NULL;
}

static const char *holds_prep(job2_t *j, int i) {
    const orders_facts *of = &g_t.st2.orders;
    const holds_facts *hf = &g_t.st2.holds;
    static char why[160];
    uint64_t grp = j->obj[i];
    if (!j->order_id) {
        if (i == 0) { j->hold_setting = 0; j->hold_nbefore = hold_area_ids(grp, j->hold_before, 64); if (j->hold_nbefore < 0) return "the army's orders are unreadable"; }
        return NULL;
    }
    if (!j->order) {
        uint64_t lk = of->lookup_fn + (uint64_t)g_slide, v = 0, own = 0; int32_t k = 0;
        if (!code_ptr(lk)) return "the order lookup is not game code";
        uint64_t o = (uint64_t)(uintptr_t)((order_lookup_fn)(uintptr_t)lk)((void *)(uintptr_t)grp, j->order_id);
        if (!o || !sq(o, &v) || v != of->vt_order + (uint64_t)g_slide) { snprintf(why, sizeof why, "order %d is not in this army's plan", j->order_id); return why; }
        if (!sq(o + of->owner_off, &own) || own != grp) { snprintf(why, sizeof why, "order %d belongs to another orders group", j->order_id); return why; }
        if (!sread(o + of->kind_off, &k, 4) || (uint32_t)k != hf->kind_area) { snprintf(why, sizeof why, "order %d is not an area-defence order (kind %d)", j->order_id, k); return why; }
        if (!sread(o + hf->setting_off, &j->hold_setting, 4)) return "the order's settings are unreadable";
        j->order = o;
    }
    int has = hold_has_state(j->order, j->provs[i]);
    if (has < 0) return "the order's states are unreadable";
    if (has) { snprintf(why, sizeof why, "state %d is already in area order %d (a click would remove it)", j->provs[i], j->order_id); return why; }
    return NULL;
}

/* engineer 85: the task forces a navy.assign names (request divs): each must be a live CTaskForce (the navy.mission type's
 * typed resolver gives it back from its handle) owned by the player (that type's owner getter), as the command's own
 * authorisation asks for every task force. Their handles become the command's list. */
static const char *navy_task_forces(job2_t *j) {
    const cmd_type *tt = stage2_find(&g_t.st2, "navy.mission");
    if (!tt || !tt->ok) return "the task force resolver is not resolved on this build";
    uint64_t of = tt->owner_fn + (uint64_t)g_slide;
    if (!code_ptr(of)) return "the task force owner getter is not game code";
    for (int k = 0; k < j->ndiv; k++) {
        uint64_t h = 0;
        if (!roundtrip(tt, j->divs[k], &h)) return "a task force is not a live CTaskForce";
        if (((owner_fn2)(uintptr_t)of)((void *)(uintptr_t)j->divs[k]) != j->player_tag) return "a task force is not the player's";
        j->divh[k] = h;
    }
    j->ndivh = j->ndiv;
    return NULL;
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
    } else if (k2 == OBJ2_SAME_PTR) { /* engineer 79: exactly the first object's class (the idea a law replaces) */
        uint64_t v2 = 0;
        if (!ct->vt_obj || !plausible(j->obj2) || !sq(j->obj2, &v2) || v2 != ct->vt_obj + (uint64_t)g_slide || j->obj2 == j->obj[0]) {
            for (int i = 0; i < j->n; i++) j->res[i] = R_BAD_OBJ2;
            snprintf(j->reason, sizeof j->reason, "obj2 is not another live %s", ct->spec->obj_cast); finish2(j); return;
        }
    } else if (k2 == OBJ2_PTR_EXACT) { /* engineer 85: a raw pointer of exactly the named class (a doctrine folder template) */
        uint64_t v2 = 0;
        if (!ct->vt_obj2 || !plausible(j->obj2) || !sq(j->obj2, &v2) || v2 != ct->vt_obj2 + (uint64_t)g_slide) {
            for (int i = 0; i < j->n; i++) j->res[i] = R_BAD_OBJ2;
            snprintf(j->reason, sizeof j->reason, "obj2 is not a live %s", ct->spec->obj2_cast); finish2(j); return;
        }
    } else if (k2 == OBJ2_CAST) { /* engineer 75: a leader / an army group, as the type's own check casts it */
        uint64_t nr = 0, ref = j->obj2 + ct->hint2, lk = ct->lookup_fn + (uint64_t)g_slide;
        if (!g_dyncast) g_dyncast = (dyncast_fn)dlsym(RTLD_DEFAULT, "__dynamic_cast");
        if (!j->obj2) {
            if (!ct->spec->obj2_optional || !sq(c->null_ref_global + (uint64_t)g_slide, &nr)) {
                for (int i = 0; i < j->n; i++) j->res[i] = R_BAD_OBJ2;
                snprintf(j->reason, sizeof j->reason, "obj2 (a %s) is required", ct->spec->obj2_cast); finish2(j); return;
            }
            h2 = nr;
        } else if (!g_dyncast || !ct->ti2 || !plausible(j->obj2) || !sq(ref + c->cref_handle_off, &h2) || !h2 || !code_ptr(lk) ||
                   (uint64_t)(uintptr_t)((resolve_fn2)(uintptr_t)lk)(&h2) != ref ||
                   (uint64_t)(uintptr_t)g_dyncast((void *)(uintptr_t)ref, (void *)(uintptr_t)(c->ti_reference + (uint64_t)g_slide),
                                                  (void *)(uintptr_t)(ct->ti2 + (uint64_t)g_slide), -1) != j->obj2) {
            for (int i = 0; i < j->n; i++) j->res[i] = R_BAD_OBJ2;
            snprintf(j->reason, sizeof j->reason, "obj2 is not a live %s", ct->spec->obj2_cast); finish2(j); return;
        }
    }
    if (ct->spec->auth_kind == AUTH_DIPLO) { /* engineer 78: no object handles; the action is built and checked whole */
        uint64_t post2 = g_t.post.post + (uint64_t)g_slide;
        const char *why = diplo_build(j, ct, gs, q, post2);
        if (why) { if (j->res[0] == R_NONE) j->res[0] = R_UNEXPECTED; snprintf(j->reason, sizeof j->reason, "%s", why); }
        finish2(j); return;
    }
    if (!strcmp(op, "units.new_army") || !strcmp(op, "units.army_group_new")) {
        const char *w = units_theatre(j);
        if (w) { for (int i = 0; i < j->n; i++) j->res[i] = R_BAD_OBJ2; snprintf(j->reason, sizeof j->reason, "%s", w); finish2(j); return; }
    }
    if (!strcmp(op, "training.create")) { /* engineer 82: the Train button's own check, can_train(deployment, body, 1, 0) */
        const training_facts *tf = &g_t.st2.train;
        uint64_t sl = (uint64_t)g_slide;
        if (!tf->ok || !code_ptr(tf->can_train + sl) || !code_ptr(tf->country_of + sl)) {
            j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "training facts are not game code"); finish2(j); return;
        }
        void *pc = ((country_of_fn)(uintptr_t)(tf->country_of + sl))(&j->player_tag);
        uint64_t dep = 0;
        if (!pc || !sq((uint64_t)(uintptr_t)pc + tf->deploy_off, &dep) || !plausible(dep)) {
            j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "the player's deployment status is unreadable"); finish2(j); return;
        }
        j->deploy = dep; /* the check itself runs per template after its round trip */
    }
    if (!strcmp(op, "construction.add")) { /* engineer 82: the template as the game's reference constructors read it */
        const construction_facts *bf = &g_t.st2.build;
        uint8_t real = 0; int32_t prov = 0, bid = 0; uint64_t vt = 0;
        const char *w = NULL;
        if (!bf->ok || !plausible(j->obj[0]) || !sq(j->obj[0], &vt) || vt != ct->vt_obj + (uint64_t)g_slide ||
            !sread(j->obj[0] + bf->tmpl_real_off, &real, 1) || !sread(j->obj[0] + bf->tmpl_prov_off, &prov, 4) ||
            !sread(j->obj[0] + bf->tmpl_id_off, &bid, 4))
            w = "the building template is not a live CBuildingTemplate";
        else if (!real) w = "the building template is not a real one (the game's reference constructors refuse it)";
        else if ((prov > 0) != (j->bld_kind == 1))
            w = prov > 0 ? "this building is placed on a province: name a province id" : "this building is placed in a state: name a state id";
        if (w) { for (int i = 0; i < j->n; i++) j->res[i] = R_BAD_OBJ2; snprintf(j->reason, sizeof j->reason, "%s", w); finish2(j); return; }
        j->bld_id = bid;
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
        if (ct->spec->auth_kind == AUTH_HANDLE &&
            (!code_ptr(owner_f) || ((owner_fn2)(uintptr_t)owner_f)((void *)(uintptr_t)j->obj[i]) != j->player_tag)) { j->res[i] = R_NOT_OWN; j->refused++; continue; }
        if (!strcmp(op, "training.create")) { /* engineer 82: a live own template: the Train button's can_train(deployment, body, 1, 0) */
            typedef unsigned char (*can_train_fn2)(void *, const void *, int32_t, int32_t);
            const training_facts *tf = &g_t.st2.train;
            if (!((can_train_fn2)(uintptr_t)(tf->can_train + (uint64_t)g_slide))((void *)(uintptr_t)j->deploy,
                                                                               (const void *)(uintptr_t)(j->obj[i] + tf->tmpl_body_off), 1, 0)) {
                j->res[i] = R_INVALID; j->refused++; continue;
            }
        }
        if (!strcmp(op, "units.transfer")) { /* the divisions (in an army or in none), then the order the UI would name */
            j->allow_free = 1;
            const char *why = orders2_divisions(j);
            int32_t oid = why ? 0 : units_transfer_order(j->obj[0]);
            if (!why && oid < 0) why = "the army's plan is unreadable";
            if (why) { j->res[i] = R_BAD_OBJ2; j->refused++; snprintf(j->reason, sizeof j->reason, "%s", why); continue; }
            j->order_id = oid; j->val[0] = oid;
        }
        if (ct->spec->auth_kind == AUTH_EVENT) {
            int unavailable = 0;
            const char *why = events_prep(j, gs, &unavailable);
            if (why) {
                j->res[i] = unavailable ? R_INVALID : R_BAD_OBJ2; j->refused++;
                if (why != j->reason) snprintf(j->reason, sizeof j->reason, "%s", why);
                continue;
            }
        }
        if (!strcmp(op, "navy.assign")) { /* engineer 85: the task forces, by the task force type's own resolver and owner */
            const char *why = navy_task_forces(j);
            if (why) { j->res[i] = R_BAD_OBJ2; j->refused++; snprintf(j->reason, sizeof j->reason, "%s", why); continue; }
        }
        if (!strcmp(ct->spec->family, "mio")) { /* engineer 84: the player's own organisation, the tree, the policy gate */
            int inv = 0;
            const char *why = mio_prep(j, i, &inv);
            if (why) { j->res[i] = inv ? R_INVALID : R_BAD_OBJ2; j->refused++; snprintf(j->reason, sizeof j->reason, "%s", why); continue; }
        }
        if (!strcmp(ct->spec->family, "holds")) { /* engineer 79: the area order and the state's place in it */
            const char *why = holds_prep(j, i);
            if (why) { j->res[i] = R_BAD_OBJ2; j->refused++; snprintf(j->reason, sizeof j->reason, "%s", why); continue; }
        }
        if (!strcmp(ct->spec->family, "orders") || !strcmp(ct->spec->family, "orders2")) {
            const char *why = orders_prep(j);
            if (!why && !strcmp(ct->spec->family, "orders2") && j->ndiv && strcmp(op, "orders2.spearhead")) why = orders2_divisions(j);
            if (!why && ct->spec->obj2 == OBJ2_FRONT) why = orders2_front(j);
            if (why) { j->res[i] = R_BAD_OBJ2; j->refused++; snprintf(j->reason, sizeof j->reason, "%s", why); continue; }
            /* the before-state of the read-back */
            const orders_facts *of = &g_t.st2.orders;
            sread(j->obj[i] + of->list_cnt[0], &j->before_top, 4);
            sread(j->obj[i] + of->list_cnt[1], &j->before_fallback, 4);
        }
        good++;
    }
    /* one command for the whole group (merge) only when every object passed: a group is never posted in part */
    if (ct->spec->one_cmd && good != j->n) {
        for (int i = 0; i < j->n; i++) if (j->res[i] == R_NONE) { j->res[i] = R_SKIPPED; }
        snprintf(j->reason, sizeof j->reason, "a group command is posted whole or not at all");
        j->post_us = 0; finish2(j); return;
    }
    if (!strcmp(ct->spec->family, "navy") && ct->spec->one_cmd && ct->spec->build == BUILD_FIELDS) {
        /* engineer 85: the selection's handles (each passed its resolver and owner above) are the command's list */
        for (int i = 0; i < j->n; i++) j->divh[i] = hs[i];
        j->ndivh = j->n;
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
                if (f->keep < 0) continue; /* engineer 85: the factory leaves it unwritten (the UI's constructor fills it) */
                if (f->kind == FK_REF || f->kind == FK_HANDLE) ok = sq((uint64_t)(uintptr_t)cmd + f->off, &cur) && cur == nullref;
                else if (f->kind == FK_I32 || f->kind == FK_TAG) { int32_t v = 1; ok = sread((uint64_t)(uintptr_t)cmd + f->off, &v, 4) && v == f->def; }
                else if (f->kind == FK_PTR) ok = sq((uint64_t)(uintptr_t)cmd + f->off, &cur) && cur == 0;
                else if (f->kind == FK_I64) { int64_t v = 1; ok = sread((uint64_t)(uintptr_t)cmd + f->off, &v, 8) && v == (int64_t)f->def; }
                else if (f->kind == FK_U8) { uint8_t v = 9; ok = sread((uint64_t)(uintptr_t)cmd + f->off, &v, 1) && v == (uint8_t)f->def; }
                else if (f->kind == FK_REF_ARRAY || f->kind == FK_I32_ARRAY) ok = 1; /* checked empty by fill_array */
                else ok = 0;
            }
            if (ok) { /* fill: our own fresh command object, never game state */
                for (int k = 0; k < ct->spec->nf && ok; k++) {
                    const cmd_field *f = &ct->spec->f[k];
                    int64_t val = f->vi > 0 ? j->val[f->vi - 1] : j->val[i];
                    if (f->keep > 0) continue;
                    if (f->kind == FK_REF) *(uint64_t *)(cmd + f->off) = k == 0 ? h : h2;
                    else if (f->kind == FK_HANDLE) *(uint64_t *)(cmd + f->off) = j->hx;
                    else if (f->kind == FK_TAG) *(int32_t *)(cmd + f->off) = j->player_tag; /* picks: the player's own tag */
                    else if (f->kind == FK_PTR) *(uint64_t *)(cmd + f->off) = j->obj[i];    /* picks: the checked object */
                    else if (f->kind == FK_PTR2) *(uint64_t *)(cmd + f->off) = j->obj2;     /* engineer 85: the checked second object */
                    else if (f->kind == FK_I64) *(int64_t *)(cmd + f->off) = val;
                    else if (f->kind == FK_REF_ARRAY) ok = fill_array(cmd, f, j->divh, j->ndivh, 8);
                    else if (f->kind == FK_I32_ARRAY) ok = fill_array(cmd, f, j->provs, j->nprov, 4);
                    else if (f->kind == FK_U8) *(uint8_t *)(cmd + f->off) = f->vi > 0 ? (uint8_t)val : (j->flag ? 1 : 0); /* orders.execute: 1 execute, 0 stop */
                    else *(int32_t *)(cmd + f->off) = (int32_t)val;
                }
                if (!ok) { /* a partly filled object: the game's own destructor frees what was allocated */
                    void **vt0 = *(void ***)cmd; ((delete_fn2)vt0[c->delete_slot / 8])(cmd);
                    j->res[i] = R_UNEXPECTED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "an array field of the fresh command is not empty or its allocator is not game code");
                    break;
                }
            }
        } else { /* the UI's own constructor, on the fresh object whose containers are still empty */
            uint64_t data = 1; int32_t size = 1;
            if (ct->spec->ctor == CTOR_ADDLINE) /* no list: the variant's handle field still holds the empty handle */
                ok = sq((uint64_t)(uintptr_t)cmd + ct->spec->f[1].off, &data) && data == nullref;
            else if (ct->spec->ctor == CTOR_LEADER || ct->spec->ctor == CTOR_GROUP_NAME || ct->spec->ctor == CTOR_EVENT_OPTION ||
                     ct->spec->ctor == CTOR_RESEARCH || ct->spec->ctor == CTOR_IDEA_ADD || ct->spec->ctor == CTOR_IDEA_REPLACE ||
                     ct->spec->ctor == CTOR_CONSTRUCTION) {
                /* no list: every handle still empty */
                for (int k = 0; k < ct->spec->nf && ok; k++)
                    if (ct->spec->f[k].kind == FK_REF) ok = sq((uint64_t)(uintptr_t)cmd + ct->spec->f[k].off, &data) && data == nullref;
            } else
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
                case CTOR_ORDER_PATH: { /* the order found by id; 1 = the UI's own edit (the AI passes 0) */
                    pdx_iarray pa = {j->provs, j->nprov, j->nprov};
                    ((ctor_order_path)(uintptr_t)ctor)(cmd, (void *)(uintptr_t)j->order, &pa, 1); break;
                }
                case CTOR_NEW_FRONT: { /* group, provinces, parent, the divisions the UI hands over, no flags, the drawer */
                    pdx_iarray pa = {j->provs, j->nprov, j->nprov};
                    pdx_array da = {(void **)(uintptr_t)j->divs, j->ndiv, j->ndiv};
                    ((ctor_new_front)(uintptr_t)ctor)(cmd, obj, &pa, (void *)(uintptr_t)j->order, &da, 0, j->blitz, 0, (void *)(uintptr_t)j->drawer);
                    break;
                }
                case CTOR_DIVS_THEATRE: { /* the selected divisions, the first one's theatre, no theater group (the UI's call) */
                    pdx_array da = {(void **)(uintptr_t)j->divs, j->ndiv, j->ndiv};
                    ((ctor_divs_theatre)(uintptr_t)ctor)(cmd, &da, obj, NULL); break;
                }
                case CTOR_LEADER: ((ctor_leader)(uintptr_t)ctor)(cmd, (void *)(uintptr_t)j->obj2, obj, 0); break;
                case CTOR_IDEA_ADD: ((ctor_idea)(uintptr_t)ctor)(cmd, &j->player_tag, obj); break;
                case CTOR_IDEA_REPLACE: ((ctor_idea2)(uintptr_t)ctor)(cmd, &j->player_tag, (void *)(uintptr_t)j->obj2, obj); break;
                case CTOR_RESEARCH: /* engineer 79: the research view's call: own tag, the template, the slot, (0, 1, 0) */
                    ((ctor_research)(uintptr_t)ctor)(cmd, &j->player_tag, obj, (int32_t)j->val[0], 0, 1, 0); break;
                case CTOR_CONSTRUCTION: { /* engineer 82: the click's call: mode 2, own tag, the reference, count 1 */
                    uint8_t ref[24] = {0};
                    uint64_t rv = g_t.st2.build.ref_vt + (uint64_t)g_slide;
                    memcpy(ref, &rv, 8); memcpy(ref + 8, &j->bld_loc, 4); memcpy(ref + 12, &j->bld_id, 4); /* tag +0x10 stays 0 */
                    ((ctor_construction)(uintptr_t)ctor)(cmd, 2, &j->player_tag, ref, 1); break;
                }
                case CTOR_AREA_STATE: /* engineer 79: the map tool's click: group, order id (0 = new), the state, settings */
                    ((ctor_area_state)(uintptr_t)ctor)(cmd, obj, j->order_id, j->provs[i], j->hold_setting); break;
                case CTOR_EVENT_OPTION: /* the window's own fields, as its option click passes them */
                    ((ctor_event_option)(uintptr_t)ctor)(cmd, j->ev_id, (void *)(uintptr_t)j->ev_event,
                                                         (const int32_t *)(uintptr_t)(j->obj[i] + g_t.st2.events.win_country_off),
                                                         (const void *)(uintptr_t)(j->obj[i] + g_t.st2.events.win_scope_off), j->ev_option);
                    break;
                case CTOR_GROUP_NAME: { /* a libc++ std::string in its short form, on our stack; the constructor copies it */
                    uint8_t sso[24] = {0};
                    sso[0] = (uint8_t)(2 * j->nname); memcpy(sso + 1, j->name, (size_t)j->nname);
                    ((ctor_group_name)(uintptr_t)ctor)(cmd, obj, sso); break;
                }
                case CTOR_GROUPS_OBJ: { /* the armies (checked, in j->divs), the shared theatre */
                    pdx_array ga = {(void **)(uintptr_t)j->divs, j->ndiv, j->ndiv};
                    ((ctor_groups_obj)(uintptr_t)ctor)(cmd, &ga, obj); break;
                }
                case CTOR_GROUPS_OBJ_I32: {
                    pdx_array ga = {(void **)(uintptr_t)j->obj, j->n, j->n};
                    ((ctor_groups_obj_i32)(uintptr_t)ctor)(cmd, &ga, (void *)(uintptr_t)j->obj2, -1); break;
                }
                case CTOR_GROUPS: {
                    pdx_array ga = {(void **)(uintptr_t)j->obj, j->n, j->n};
                    ((ctor_array)(uintptr_t)ctor)(cmd, &ga); break;
                }
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
            } else if (ok && (ct->spec->ctor == CTOR_ORDER_PATH || ct->spec->ctor == CTOR_NEW_FRONT)) {
                /* the group's handle, the order id, the provinces in order (and for a front: the divisions, no flags) */
                int nf = ct->spec->ctor == CTOR_NEW_FRONT;
                uint64_t hg = 0, pd = 0; int32_t pn = -1, idv = -1; uint8_t fl[3] = {9, 9, 9}, top = 9;
                ok = sq((uint64_t)(uintptr_t)cmd, &cvp) && cvp == ct->vtable + (uint64_t)g_slide &&
                     sq((uint64_t)(uintptr_t)cmd + ct->spec->f[1].off, &hg) && hg == h &&
                     sread((uint64_t)(uintptr_t)cmd + ct->spec->f[2].off, &idv, 4) && idv == j->order_id &&
                     sq((uint64_t)(uintptr_t)cmd + ct->spec->f[0].off, &pd) && sread((uint64_t)(uintptr_t)cmd + ct->spec->f[0].count_off, &pn, 4) &&
                     pn == j->nprov;
                for (int e = 0; ok && e < j->nprov; e++) { int32_t x = 0; ok = sread(pd + 4 * (uint64_t)e, &x, 4) && x == j->provs[e]; }
                if (ok && nf) {
                    uint64_t dd = 0; int32_t dn = -1;
                    ok = sq((uint64_t)(uintptr_t)cmd + ct->spec->f[3].off, &dd) && sread((uint64_t)(uintptr_t)cmd + ct->spec->f[3].count_off, &dn, 4) &&
                         dn == j->ndiv && sread((uint64_t)(uintptr_t)cmd + 0x6c, fl, 3) && !fl[0] && fl[1] == (j->blitz ? 1 : 0) && !fl[2];
                } else if (ok) ok = sread((uint64_t)(uintptr_t)cmd + 0x48, &top, 1) && top == 1;
                if (!ok) {
                    /* what differed, for the session notes (s45b, 27 Nov 2026: a new front was refused here) */
                    uint64_t dd2 = 0; int32_t dn2 = -1, pn2 = -1, id2 = -1; uint64_t hg2 = 0; uint8_t f2[3] = {9, 9, 9};
                    sq((uint64_t)(uintptr_t)cmd + ct->spec->f[1].off, &hg2); sread((uint64_t)(uintptr_t)cmd + ct->spec->f[2].off, &id2, 4);
                    sread((uint64_t)(uintptr_t)cmd + ct->spec->f[0].count_off, &pn2, 4);
                    if (nf) { sq((uint64_t)(uintptr_t)cmd + ct->spec->f[3].off, &dd2); sread((uint64_t)(uintptr_t)cmd + ct->spec->f[3].count_off, &dn2, 4); sread((uint64_t)(uintptr_t)cmd + 0x6c, f2, 3); }
                    j->res[i] = R_UNEXPECTED; j->unexplained = 1;
                    snprintf(j->reason, sizeof j->reason, "the constructor built an unexpected payload (group %s, id %d of %d, provinces %d of %d, divisions %d of %d, flags %u/%u/%u)",
                             hg2 == h ? "ok" : "differs", id2, j->order_id, pn2, j->nprov, dn2, j->ndiv, f2[0], f2[1], f2[2]);
                    break;
                }
            } else if (ok && ct->spec->ctor == CTOR_EVENT_OPTION) { /* engineer 76: id, event handle, country, option */
                const events_facts *e = &g_t.st2.events;
                int32_t a = 0, b = 0, d = -1; uint64_t hv = 0;
                ok = sq((uint64_t)(uintptr_t)cmd, &cvp) && cvp == ct->vtable + (uint64_t)g_slide &&
                     sread((uint64_t)(uintptr_t)cmd + e->cmd_id_off, &a, 4) && a == j->ev_id &&
                     sq((uint64_t)(uintptr_t)cmd + e->ev_handle_off, &hv) && hv == j->ev_handle &&
                     sread((uint64_t)(uintptr_t)cmd + e->cmd_country_off, &b, 4) && b == j->player_tag &&
                     sread((uint64_t)(uintptr_t)cmd + e->cmd_option_off, &d, 4) && d == j->ev_option;
                if (!ok) { j->res[i] = R_UNEXPECTED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "the constructor built an unexpected payload"); break; }
            } else if (ok && (ct->spec->ctor == CTOR_IDEA_ADD || ct->spec->ctor == CTOR_IDEA_REPLACE)) { /* engineer 79 */
                int32_t tg = 0; uint64_t a = 0, b = 0; int rep = ct->spec->ctor == CTOR_IDEA_REPLACE;
                ok = sq((uint64_t)(uintptr_t)cmd, &cvp) && cvp == ct->vtable + (uint64_t)g_slide &&
                     sread((uint64_t)(uintptr_t)cmd + 0x24, &tg, 4) && tg == j->player_tag &&
                     sq((uint64_t)(uintptr_t)cmd + 0x28, &a) && a == (rep ? j->obj2 : j->obj[i]) &&
                     (!rep || (sq((uint64_t)(uintptr_t)cmd + 0x30, &b) && b == j->obj[i]));
                if (!ok) { j->res[i] = R_UNEXPECTED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "the constructor built an unexpected payload"); break; }
            } else if (ok && ct->spec->ctor == CTOR_CONSTRUCTION) { /* engineer 82: tag, the reference, count 1, mode 2 */
                int32_t tg = 0, loc = 0, bid = 0, rt = -1, cnt = 0; uint64_t rv = 0; uint8_t md = 9;
                ok = sq((uint64_t)(uintptr_t)cmd, &cvp) && cvp == ct->vtable + (uint64_t)g_slide &&
                     sread((uint64_t)(uintptr_t)cmd + 0x24, &tg, 4) && tg == j->player_tag &&
                     sq((uint64_t)(uintptr_t)cmd + 0x28, &rv) && rv == g_t.st2.build.ref_vt + (uint64_t)g_slide &&
                     sread((uint64_t)(uintptr_t)cmd + 0x30, &loc, 4) && loc == j->bld_loc &&
                     sread((uint64_t)(uintptr_t)cmd + 0x34, &bid, 4) && bid == j->bld_id &&
                     sread((uint64_t)(uintptr_t)cmd + 0x38, &rt, 4) && rt == 0 &&
                     sread((uint64_t)(uintptr_t)cmd + 0x40, &cnt, 4) && cnt == 1 &&
                     sread((uint64_t)(uintptr_t)cmd + 0x44, &md, 1) && md == 2;
                if (!ok) { j->res[i] = R_UNEXPECTED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "the constructor built an unexpected payload"); break; }
            } else if (ok && ct->spec->ctor == CTOR_RESEARCH) { /* engineer 79: tag, template, slot, bytes 0/1/0 */
                int32_t tg = 0, sl = -1; uint64_t tp = 0; uint8_t b[3] = {9, 9, 9};
                ok = sq((uint64_t)(uintptr_t)cmd, &cvp) && cvp == ct->vtable + (uint64_t)g_slide &&
                     sread((uint64_t)(uintptr_t)cmd + 0x24, &tg, 4) && tg == j->player_tag &&
                     sq((uint64_t)(uintptr_t)cmd + 0x28, &tp) && tp == j->obj[i] &&
                     sread((uint64_t)(uintptr_t)cmd + 0x30, &sl, 4) && sl == (int32_t)j->val[0] &&
                     sread((uint64_t)(uintptr_t)cmd + 0x34, b, 3) && b[0] == 0 && b[1] == 1 && b[2] == 0;
                if (!ok) { j->res[i] = R_UNEXPECTED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "the constructor built an unexpected payload"); break; }
            } else if (ok && ct->spec->ctor == CTOR_AREA_STATE) { /* engineer 79: exactly one click's payload */
                uint64_t hg = 0, d = 0; int32_t oid = -1, cnt = -1, x = 0, st = -1; uint8_t b[2] = {9, 9};
                ok = sq((uint64_t)(uintptr_t)cmd, &cvp) && cvp == ct->vtable + (uint64_t)g_slide &&
                     sq((uint64_t)(uintptr_t)cmd + ct->spec->f[1].off, &hg) && hg == h &&
                     sread((uint64_t)(uintptr_t)cmd + ct->spec->f[2].off, &oid, 4) && oid == j->order_id &&
                     sq((uint64_t)(uintptr_t)cmd + ct->spec->f[0].off, &d) && sread((uint64_t)(uintptr_t)cmd + ct->spec->f[0].count_off, &cnt, 4) &&
                     cnt == 1 && sread(d, &x, 4) && x == j->provs[i] &&
                     sread((uint64_t)(uintptr_t)cmd + ct->spec->f[4].off, b, 2) && b[0] == 0 && b[1] == 1 &&
                     sread((uint64_t)(uintptr_t)cmd + ct->spec->f[3].off, &st, 4) && st == j->hold_setting;
                if (!ok) { j->res[i] = R_UNEXPECTED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "the constructor built an unexpected payload"); break; }
            } else if (ok && ct->spec->ctor >= CTOR_DIVS_THEATRE) { /* engineer 75: army structure */
                int filtered = 0;
                const char *bad = sq((uint64_t)(uintptr_t)cmd, &cvp) && cvp == ct->vtable + (uint64_t)g_slide
                                      ? units_payload(j, cmd, h, h2, nullref, &filtered) : "vtable changed";
                if (bad) { j->res[i] = R_UNEXPECTED; j->unexplained = 1; snprintf(j->reason, sizeof j->reason, "the constructor built an unexpected payload: %s", bad); break; }
                if (filtered) { /* the game's own filter: refused like a failed check, nothing posted */
                    void **vt0 = *(void ***)cmd; ((delete_fn2)vt0[c->delete_slot / 8])(cmd);
                    j->res[i] = R_INVALID; j->refused++;
                    snprintf(j->reason, sizeof j->reason, "the game's own selection filter dropped %d division(s) (not movable now)", filtered);
                    continue;
                }
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

static int parse_list_i32(const char *s, int32_t *out, int max) {
    int n = 0;
    while (*s && n < max) {
        char *e; long v = strtol(s, &e, 10);
        if (e == s || v <= 0 || v > 1000000) return -1;
        out[n++] = (int32_t)v;
        if (*e == ',') e++; else if (*e) return -1;
        s = e;
    }
    return *s ? -1 : n;
}

static int be_post2(const char *op, const char *objs, const char *vals, const char *obj2, const char *provs, const char *divs,
                    int64_t flag, long id, char *out, size_t n, int *unexplained) {
    const cmd_type *ct = stage2_find(&g_t.st2, op);
    if (!ct || !ct->ok) { snprintf(out, n, "\"op\":\"%s\",\"result\":\"refused\",\"reason\":\"type not resolved on this build\"", op); logj("\"event\":\"post2\",\"id\":%ld,%s", id, out); return 0; }
    job2_t *j = (job2_t *)calloc(1, sizeof *j);
    if (!j) return 0;
    j->refs = 2; j->ct = ct; j->id = id; j->flag = flag;
    j->n = parse_list_u64(objs, j->obj, J2_MAX);
    int nv = vals && *vals ? parse_list_i64(vals, j->val, J2_MAX) : 0;
    const char *why = NULL;
    if (!strncmp(op, "diplomacy.", 10)) {
        /* engineer 78: no object (objs = "0", the channel's one placeholder); vals = the receiving country's tag id (the
         * shown country of the diplomacy view) */
        if (j->n != 1 || j->obj[0] != 0) why = "diplomacy ops name no object: objs = 0 (vals: the receiving country's tag id)";
        else if (nv != 1 || j->val[0] < 1 || j->val[0] > 100000) why = "vals: the receiving country's tag id";
        else if (flag || (obj2 && *obj2) || (provs && *provs) || (divs && *divs)) why = "diplomacy ops take no flag, obj2, provs or divs";
        else { j->n = 1; j->obj[0] = 0; }
        if (why) { snprintf(out, n, "\"op\":\"%s\",\"result\":\"refused_by_policy\",\"reason\":\"%s\"", op, why); free(j); logj("\"event\":\"post2\",\"id\":%ld,%s", id, out); return 0; }
        nv = 0; /* consumed */
    } else if (j->n < 1) why = "objs: comma-separated hex addresses";
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
    } else if (!strncmp(op, "orders.", 7)) {
        /* battle plans: ONE orders group, vals = the order id (orders.execute: 0 = the whole plan), flag = execute 1 /
         * stop 0 (orders.execute only), provs = the province ids in drawing order (set_path, new_front), obj2 = the
         * drawing group (new_front, optional) */
        int wants_provs = !strcmp(op, "orders.set_path") || !strcmp(op, "orders.new_front");
        if (j->n != 1) why = "orders: exactly one orders group per request";
        else if (nv != 1 || j->val[0] < (strcmp(op, "orders.execute") ? 1 : 0) || j->val[0] > 100000000) why = "vals: one order id";
        else if (strcmp(op, "orders.execute") && flag) why = "flag is for orders.execute only (1 execute, 0 stop)";
        else if (flag != 0 && flag != 1) why = "flag: 1 execute, 0 stop";
        else if (wants_provs && (j->nprov = parse_list_i32(provs ? provs : "", j->provs, J2_PROVS)) < 1)
            why = "provs: 1 to 1024 comma-separated province ids in drawing order";
        else if (!wants_provs && provs && *provs) why = "this op takes no provs";
        else {
            j->order_id = (int32_t)j->val[0];
            for (int a = 0; !why && a < j->nprov; a++) for (int b = a + 1; b < j->nprov; b++) if (j->provs[a] == j->provs[b]) why = "a province is listed twice";
        }
        if (!why && obj2 && *obj2) {
            uint64_t o2[1];
            if (strcmp(op, "orders.new_front")) why = "obj2 is for orders.new_front only (the drawing group)";
            else if (parse_list_u64(obj2, o2, 1) != 1) why = "obj2: one hex address"; else j->obj2 = o2[0];
        }
    } else if (!strncmp(op, "orders2.", 8)) {
        /* engineer 74: the rest of the battle plan. ONE orders group. vals by op:
         *   new_root: section id, from, to (0..100000), split (0/1); obj2 = the CFront; divs = the divisions to assign
         *   assign: order id; divs (1..)      unassign: no vals; divs (1..)
         *   fallback: no vals; provs (the line, drawing order); divs (0..)
         *   edit_root: order id, from, to     invasion_target / paradrop_target: order id, province id
         *   spearhead: parent order id; provs (as orders.new_front; the parent's free divisions are handed over) */
        int64_t lim_hi = 100000;
        int want = !strcmp(op, "orders2.new_root") ? 4 : !strcmp(op, "orders2.edit_root") ? 3 :
                   (!strcmp(op, "orders2.invasion_target") || !strcmp(op, "orders2.paradrop_target")) ? 2 :
                   (!strcmp(op, "orders2.assign") || !strcmp(op, "orders2.spearhead")) ? 1 : 0;
        int wants_provs = !strcmp(op, "orders2.fallback") || !strcmp(op, "orders2.spearhead");
        int wants_divs = !strcmp(op, "orders2.assign") || !strcmp(op, "orders2.unassign");
        int may_divs = wants_divs || !strcmp(op, "orders2.new_root") || !strcmp(op, "orders2.fallback");
        if (j->n != 1) why = "orders2: exactly one orders group per request";
        else if (nv != want) why = "vals: the wrong number of values for this op";
        else if (flag) why = "orders2 ops take no flag";
        else if (want >= 1 && strcmp(op, "orders2.new_root") && (j->val[0] < 1 || j->val[0] > 100000000)) why = "vals[0]: an order id";
        else if (!strcmp(op, "orders2.new_root") && (j->val[0] < 0 || j->val[0] > 100000000 || j->val[1] < 0 || j->val[1] > lim_hi ||
                                                    j->val[2] < 0 || j->val[2] > lim_hi || j->val[1] >= j->val[2] || (j->val[3] != 0 && j->val[3] != 1)))
            why = "new_root vals: section id, from < to within 0..100000, split 0/1";
        else if (!strcmp(op, "orders2.edit_root") && (j->val[1] < 0 || j->val[1] > lim_hi || j->val[2] < 0 || j->val[2] > lim_hi || j->val[1] >= j->val[2]))
            why = "edit_root vals: order id, from < to within 0..100000";
        else if (want == 2 && (j->val[1] < 1 || j->val[1] > 1000000)) why = "vals[1]: a province id";
        else if (wants_provs && (j->nprov = parse_list_i32(provs ? provs : "", j->provs, J2_PROVS)) < 1)
            why = "provs: 1 to 1024 comma-separated province ids in drawing order";
        else if (!wants_provs && provs && *provs) why = "this op takes no provs";
        else if (divs && *divs && !may_divs) why = "this op takes no divs";
        else if (may_divs && divs && *divs && (j->ndiv = parse_list_u64(divs, j->divs, J2_DIVS)) < 1) why = "divs: comma-separated hex division addresses";
        else if (wants_divs && j->ndiv < 1) why = "divs: at least one division";
        else {
            j->order_id = want >= 1 && strcmp(op, "orders2.new_root") ? (int32_t)j->val[0] : 0;
            j->blitz = !strcmp(op, "orders2.spearhead");
            for (int a = 0; !why && a < j->nprov; a++) for (int b = a + 1; b < j->nprov; b++) if (j->provs[a] == j->provs[b]) why = "a province is listed twice";
        }
        if (!why && obj2 && *obj2 && strcmp(op, "orders2.new_root")) why = "obj2 is for orders2.new_root only (the CFront)";
        if (!why && !strcmp(op, "orders2.new_root")) { uint64_t o2[1]; if (parse_list_u64(obj2 ? obj2 : "", o2, 1) != 1) why = "obj2: the CFront's hex address"; else j->obj2 = o2[0]; }
    } else if (!strncmp(op, "units.", 6)) {
        /* engineer 75: army structure. objs / divs / vals / obj2 by op:
         *   new_army: objs = the first division (whose theatre the UI reads), divs = all divisions (the first = objs)
         *   transfer: objs = the target army, divs = the divisions (1..512)
         *   leader: objs = the army / army group, obj2 = the leader (absent = unassign)
         *   rename: objs = the army / army group, vals = the new name's character codes (1..22, printable ASCII)
         *   army_group_new: objs = the armies (2..32); to_army_group: objs = armies, obj2 = the army group;
         *   from_army_group: objs = armies; delete_group: objs = groups (one command each) */
        int is = 0;
        if (provs && *provs) why = "units ops take no provs";
        else if (flag) why = "units ops take no flag";
        else if (!strcmp(op, "units.new_army") || !strcmp(op, "units.transfer")) {
            is = 1;
            if (j->n != 1) why = "exactly one object (the first division / the target army)";
            else if (nv) why = "this op takes no vals";
            else if ((j->ndiv = parse_list_u64(divs ? divs : "", j->divs, J2_DIVS)) < 1) why = "divs: 1 to 512 comma-separated division addresses";
            else if (!strcmp(op, "units.new_army") && j->divs[0] != j->obj[0]) why = "new_army: objs names the first of divs";
            for (int a = 0; !why && a < j->ndiv; a++) for (int b = a + 1; b < j->ndiv; b++) if (j->divs[a] == j->divs[b]) why = "a division is listed twice";
        } else if (!strcmp(op, "units.leader")) {
            is = 1;
            if (j->n != 1 || nv) why = "leader: one army or army group, no vals";
        } else if (!strcmp(op, "units.rename")) {
            is = 1;
            if (j->n != 1) why = "rename: one army or army group";
            else if (nv < 1 || nv > 22) why = "vals: the new name's 1..22 character codes";
            else for (int k = 0; k < nv; k++) {
                if (j->val[k] < 32 || j->val[k] > 126 || j->val[k] == '"' || j->val[k] == '\\') { why = "a name character outside printable ASCII"; break; }
                j->name[k] = (char)j->val[k];
            }
            j->nname = nv;
        } else if (!strcmp(op, "units.army_group_new")) {
            is = 1;
            if (j->n < 2 || nv) why = "army_group_new: 2..32 armies, no vals";
            else { for (int k = 0; k < j->n; k++) j->divs[k] = j->obj[k]; j->ndiv = j->n; } /* the job resolves the theatre */
        } else if (!strcmp(op, "units.to_army_group") || !strcmp(op, "units.from_army_group") || !strcmp(op, "units.delete_group")) {
            is = 1;
            if (nv) why = "this op takes no vals";
        }
        if (!is && !why) why = "unknown units op";
        if (!why && divs && *divs && strcmp(op, "units.new_army") && strcmp(op, "units.transfer")) why = "this op takes no divs";
        if (!why && obj2 && *obj2) {
            uint64_t o2[1];
            if (strcmp(op, "units.leader") && strcmp(op, "units.to_army_group")) why = "obj2 is for units.leader / units.to_army_group";
            else if (parse_list_u64(obj2, o2, 1) != 1) why = "obj2: one hex address"; else j->obj2 = o2[0];
        }
        if (!why && !strcmp(op, "units.to_army_group") && !j->obj2) why = "obj2: the army group";
    } else if (!strncmp(op, "politics.", 9)) {
        /* engineer 79: objs = ONE idea (exact CIdea); replace: obj2 = the slot's current idea (same class) */
        int rep = !strcmp(op, "politics.replace");
        if (j->n != 1) why = "politics: exactly one idea";
        else if (nv || flag || (provs && *provs) || (divs && *divs)) why = "politics ops take no vals, flag, provs or divs";
        else if (!rep && obj2 && *obj2) why = "politics.add takes no obj2";
        else if (rep) { uint64_t o2[1]; if (parse_list_u64(obj2 ? obj2 : "", o2, 1) != 1) why = "obj2: the current idea's hex address"; else j->obj2 = o2[0]; }
    } else if (!strncmp(op, "research.", 9)) {
        /* engineer 79: objs = ONE technology template (exact class); vals = the research slot (0..31: the game's own check
         * reads the country's slot count) */
        if (j->n != 1) why = "research.start: exactly one technology";
        else if (nv != 1 || j->val[0] < 0 || j->val[0] > 31) why = "vals: one research slot index (0..31)";
        else if (flag || (obj2 && *obj2) || (provs && *provs) || (divs && *divs)) why = "research.start takes no flag, obj2, provs or divs";
    } else if (!strncmp(op, "training.", 9)) {
        /* engineer 82: training (the deployment view).
         *   create:    objs = own division templates (1..8, exact CReferencedDivisionTemplate); no vals
         *   series:    objs = own training queues; vals = the series amount each (0 = forever .. 1000)
         *   add_lines: objs = ONE own training queue; vals = the lines to add (1..24: Shift-clicks of at most 5 each)
         *   location:  objs = own training queues (1..32); vals = ONE province id (1..65535) for all
         *   deploy, remove: objs = own training queues (1..32); no vals */
        if (flag || (obj2 && *obj2) || (provs && *provs) || (divs && *divs)) why = "training ops take no flag, obj2, provs or divs";
        else if (!strcmp(op, "training.create")) { if (j->n > 8) why = "training.create: 1..8 templates"; else if (nv) why = "training.create takes no vals"; }
        else if (!strcmp(op, "training.series")) {
            if (nv != j->n) why = "vals: the series amount for each queue";
            else for (int k = 0; k < j->n; k++) if (j->val[k] < 0 || j->val[k] > 1000) why = "series amount outside 0..1000";
        } else if (!strcmp(op, "training.add_lines")) {
            if (j->n != 1) why = "training.add_lines: exactly one queue";
            else if (nv != 1 || j->val[0] < 1 || j->val[0] > 24) why = "vals: the lines to add (1..24)";
            else {
                uint64_t q = j->obj[0]; int64_t left = j->val[0]; int k = 0;
                for (; left > 0 && k < 5; k++) { j->obj[k] = q; j->val[k] = left > 5 ? 5 : left; left -= j->val[k]; }
                j->n = k;
            }
        } else if (!strcmp(op, "training.location")) {
            if (nv != 1 || j->val[0] < 1 || j->val[0] > 65535) why = "vals: one province id (1..65535)";
        } else if (!strcmp(op, "training.deploy") || !strcmp(op, "training.remove")) { if (nv) why = "this op takes no vals"; }
        else why = "unknown training op";
        nv = 0;
    } else if (!strncmp(op, "construction.", 13)) {
        /* engineer 82: construction.
         *   add:    objs = ONE building template (exact CBuildingTemplate); vals = the location id, its kind (0 a state id, 1 a
         *           province id: the module checks it against the template as the game's reference constructors decide),
         *           the clicks (1..10: one command each, count 1)
         *   remove: objs = own construction lines (1..32); vals = the levels each loses (1..1000: the line's own amount
         *           takes the whole line)
         *   move:   objs = ONE own construction line; vals = the new index (0..2047), the "placed by hand" byte (0/1) */
        if (flag || (obj2 && *obj2) || (provs && *provs) || (divs && *divs)) why = "construction ops take no flag, obj2, provs or divs";
        else if (!strcmp(op, "construction.add")) {
            if (j->n != 1) why = "construction.add: exactly one building template";
            else if (nv != 3 || j->val[0] < 1 || j->val[0] > 1000000 || (j->val[1] != 0 && j->val[1] != 1) || j->val[2] < 1 || j->val[2] > 10)
                why = "vals: the location id, its kind (0 state, 1 province), the clicks (1..10)";
            else {
                uint64_t t0 = j->obj[0];
                j->bld_loc = (int32_t)j->val[0]; j->bld_kind = (int32_t)j->val[1];
                j->n = (int)j->val[2];
                for (int k = 0; k < j->n; k++) j->obj[k] = t0; /* one click each, the same building */
            }
        } else if (!strcmp(op, "construction.remove")) {
            if (nv != j->n) why = "vals: the levels each line loses";
            else for (int k = 0; k < j->n; k++) if (j->val[k] < 1 || j->val[k] > 1000) why = "levels outside 1..1000";
        } else if (!strcmp(op, "construction.move")) {
            if (j->n != 1) why = "construction.move: exactly one line";
            else if (nv != 2 || j->val[0] < 0 || j->val[0] > 2047 || (j->val[1] != 0 && j->val[1] != 1)) why = "vals: the new index (0..2047), the placed-by-hand byte (0/1)";
        } else why = "unknown construction op";
        nv = 0;
    } else if (!strncmp(op, "holds.", 6)) {
        /* engineer 79: an area-defence order, one map click per state. objs = ONE army (or army group); vals = the area
         * order's id (0 = a new area-defence order: exactly one state, its first click) then the state ids (one command
         * each, in order). The states are the MCP's own-controlled reads; the module checks the army, the order and that
         * no state is in the order already (the click toggles) */
        if (j->n != 1) why = "holds.area_defense: exactly one army";
        else if (nv < 2 || nv > 9) why = "vals: the area order's id (0 = new) then 1..8 state ids";
        else if (j->val[0] < 0 || j->val[0] > 100000000) why = "vals[0]: an area order's id or 0 (a new order)";
        else if (j->val[0] == 0 && nv != 2) why = "a new area order takes exactly one state (its first click): add the others to its id";
        else if (flag || (obj2 && *obj2) || (provs && *provs) || (divs && *divs)) why = "holds ops take no flag, obj2, provs or divs";
        else {
            uint64_t g = j->obj[0];
            j->order_id = (int32_t)j->val[0];
            j->nprov = nv - 1;
            for (int k = 0; k < j->nprov && !why; k++) {
                if (j->val[k + 1] < 1 || j->val[k + 1] > 100000) why = "a state id is outside 1..100000";
                j->provs[k] = (int32_t)j->val[k + 1];
                for (int e = 0; e < k && !why; e++) if (j->provs[e] == j->provs[k]) why = "a state is listed twice (a second click would remove it)";
            }
            j->n = j->nprov; /* one command per state, each for the same army */
            for (int k = 0; k < j->n; k++) j->obj[k] = g;
        }
    } else if (!strncmp(op, "mio.", 4)) {
        /* engineer 84: the organisations.
         *   queue_trait / unlock_trait: objs = ONE own organisation; vals = the trait's token (from the mios read)
         *   policy:      objs = ONE own organisation; vals = the policy item's token (the attached policy's own item detaches)
         *   line:        objs = own equipment production lines (1..32); obj2 = ONE own organisation, or none (produce without) */
        if (flag || (provs && *provs) || (divs && *divs)) why = "mio ops take no flag, provs or divs";
        else if (!strcmp(op, "mio.line")) {
            uint64_t o2[1];
            if (nv) why = "mio.line takes no vals";
            else if (obj2 && *obj2 && parse_list_u64(obj2, o2, 1) != 1) why = "obj2: one hex address (an own organisation) or none";
            else j->obj2 = obj2 && *obj2 ? o2[0] : 0;
        } else if (!strcmp(op, "mio.queue_trait") || !strcmp(op, "mio.unlock_trait") || !strcmp(op, "mio.policy")) {
            if (j->n != 1) why = "exactly one organisation";
            else if (obj2 && *obj2) why = "this op takes no obj2";
            else if (nv != 1 || j->val[0] <= 0 || j->val[0] > 0x7fffffff) why = "vals: one token from the organisations read";
        } else why = "unknown mio op";
        nv = 0;
    } else if (!strncmp(op, "navy.", 5)) {
        /* engineer 85: task forces and fleets, as the navy view posts them.
         *   mission:       objs = own task forces (1..32, ONE command, the mission bar's selection); vals = the mission
         *                  code of a mission button (0 hold, 1 patrol, 2 strike force, 3 convoy raiding, 4 convoy escort,
         *                  9 invasion support; 8 = the reserve: no button; 5/6 mines and 7 training: refused, see below)
         *   merge:         objs = own task forces (2..32, ONE command: the toolbar's merge of the selection)
         *   add_region / remove_region: objs = ONE own fleet; vals = the strategic region id
         *   leader:        objs = ONE own fleet; obj2 = the admiral (CNavyLeader) or none (unassign)
         *   assign:        objs = ONE own fleet (the receiving card); divs = own task forces (1..32) */
        uint64_t o2[1];
        if (flag || (provs && *provs)) why = "navy ops take no flag or provs";
        else if (divs && *divs && strcmp(op, "navy.assign")) why = "only navy.assign takes divs (the task forces)";
        else if (obj2 && *obj2 && strcmp(op, "navy.leader")) why = "only navy.leader takes obj2 (the admiral)";
        else if (!strcmp(op, "navy.mission")) {
            if (nv != 1 || j->val[0] < 0 || j->val[0] > 9 || j->val[0] == 8) why = "vals: one mission code of a mission button (0..7, 9)";
            /* the mine buttons are enabled by technology, a UI rule the command's own "can do" (code <= 9) does not
             * repeat: the module never posts them (FAIR: only what the UI offers); the mouse path serves them */
            else if (j->val[0] == 5 || j->val[0] == 6) why = "mine missions stay on the mouse path (their button's technology rule is the UI's)";
            /* training: the button sets +0x44 under a game-state setting (1.20 0x101dc9a48: gamestate +0x4d0, its slot
             * 0x30) the module does not read: the mouse path serves it */
            else if (j->val[0] == 7) why = "training stays on the mouse path (the button's +0x44 depends on a game setting)";
            else for (int i = 1; i < j->n; i++) j->val[i] = j->val[0];
        } else if (!strcmp(op, "navy.merge")) {
            if (nv) why = "navy.merge takes no vals";
            else if (j->n < 2) why = "a merge takes at least two task forces";
        } else if (!strcmp(op, "navy.add_region") || !strcmp(op, "navy.remove_region")) {
            if (j->n != 1) why = "exactly one fleet";
            else if (nv != 1 || j->val[0] < 1 || j->val[0] > 100000) why = "vals: one strategic region id";
            else if (!strcmp(op, "navy.remove_region")) { j->provs[0] = (int32_t)j->val[0]; j->nprov = 1; }
        } else if (!strcmp(op, "navy.leader")) {
            if (j->n != 1) why = "exactly one fleet";
            else if (nv) why = "navy.leader takes no vals";
            else if (obj2 && *obj2 && parse_list_u64(obj2, o2, 1) != 1) why = "obj2: one hex address (an own admiral) or none";
            else j->obj2 = obj2 && *obj2 ? o2[0] : 0;
        } else if (!strcmp(op, "navy.assign")) {
            if (j->n != 1) why = "exactly one receiving fleet";
            else if (nv) why = "navy.assign takes no vals";
            else if ((j->ndiv = parse_list_u64(divs ? divs : "", j->divs, J2_MAX)) < 1) why = "divs: 1..32 comma-separated hex task force addresses";
            else for (int a = 0; !why && a < j->ndiv; a++) for (int b = a + 1; b < j->ndiv; b++) if (j->divs[a] == j->divs[b]) why = "a task force is listed twice";
        } else why = "unknown navy op";
        nv = 0;
    } else if (!strncmp(op, "projects.", 9)) {
        /* engineer 85: the experimental facilities tab.
         *   start:    objs = ONE own facility (NProject::CProgram); obj2 = the project (NProject::CProject)
         *   stop:     objs = ONE own project
         *   attach:   objs = ONE own facility; obj2 = the scientist (CCharacter); the flag is the UI's 1
         *   unattach: objs = ONE own scientist (CCharacter); obj2 = the facility */
        int stop = !strcmp(op, "projects.stop");
        if (j->n != 1) why = "projects: exactly one object";
        else if (nv || flag || (provs && *provs) || (divs && *divs)) why = "projects ops take no vals, flag, provs or divs";
        else if (stop && obj2 && *obj2) why = "projects.stop takes no obj2";
        else if (!stop) { uint64_t o2[1]; if (parse_list_u64(obj2 ? obj2 : "", o2, 1) != 1) why = "obj2: one hex address"; else j->obj2 = o2[0]; }
        if (!why && !strcmp(op, "projects.attach")) j->flag = 1;
    } else if (!strncmp(op, "doctrine.", 9)) {
        /* engineer 85: the officer corps view's doctrine picks.
         *   grand: objs = ONE grand doctrine template (exact class); no vals, no obj2
         *   sub:   objs = ONE subdoctrine template (exact class); obj2 = the folder template (exact class); vals = the
         *          track's place in the folder's grand doctrine (0..31: the game's own check reads the track count) */
        int sub = !strcmp(op, "doctrine.sub");
        if (j->n != 1) why = "doctrine: exactly one doctrine";
        else if (flag || (provs && *provs) || (divs && *divs)) why = "doctrine ops take no flag, provs or divs";
        else if (!sub && (nv || (obj2 && *obj2))) why = "doctrine.grand takes no vals or obj2";
        else if (sub && (nv != 1 || j->val[0] < 0 || j->val[0] > 31)) why = "vals: the track's place (0..31)";
        else if (sub) { uint64_t o2[1]; if (parse_list_u64(obj2 ? obj2 : "", o2, 1) != 1) why = "obj2: the folder template's hex address"; else j->obj2 = o2[0]; }
    } else if (!strncmp(op, "picks.", 6)) {
        /* engineer 75: the player's own picks. focus: objs = ONE CNationalFocus of the own tree (exact vtable; the game's
         * own "can start" with the player's country decides) */
        if (nv) why = "this op takes no vals";
        else if (j->n != 1) why = "picks.focus: exactly one focus";
        else if (obj2 && *obj2) why = "this op takes no obj2";
    } else if (!strncmp(op, "decisions.", 10)) {
        /* engineer 76: objs = ONE CDecision (exact vtable; the game's own "can do" with the player's country decides) */
        if (nv) why = "this op takes no vals";
        else if (j->n != 1) why = "decisions.take: exactly one decision";
        else if (flag || (obj2 && *obj2) || (provs && *provs)) why = "decisions.take takes no flag, obj2 or provs";
    } else if (!strncmp(op, "events.", 7)) {
        /* engineer 76: an event answer. objs = ONE open event window (CEventWindow); vals = the option's place among the
         * shown options (0..7) and how many the window shows (1..8: the module's option test must agree) */
        if (j->n != 1) why = "events.answer: exactly one event window";
        else if (nv != 2 || j->val[0] < 0 || j->val[0] > 7 || j->val[1] < 1 || j->val[1] > 8 || j->val[0] >= j->val[1])
            why = "vals: the option's shown place (0..7) and the shown option count (1..8)";
        else if (flag || (obj2 && *obj2) || (provs && *provs)) why = "events.answer takes no flag, obj2 or provs";
    } else if (nv) why = "this op takes no vals";
    if (!why && divs && *divs && strncmp(op, "orders2.", 8) && strncmp(op, "units.", 6) && strcmp(op, "navy.assign")) why = "only orders2, units and navy.assign take divs";
    if (!why && !strcmp(op, "airwing.merge") && j->n < 2) why = "a merge takes at least two wings";
    if (!why && (!strcmp(op, "air.transfer") || !strcmp(op, "air.enable_mission") || !strcmp(op, "airwing.attach"))) {
        uint64_t o2[1];
        if (parse_list_u64(obj2 ? obj2 : "", o2, 1) != 1) why = "obj2: one hex address"; else j->obj2 = o2[0];
    }
    for (int a = 0; !why && strncmp(op, "holds.", 6) && strcmp(op, "construction.add") && strcmp(op, "training.add_lines") && a < j->n; a++) for (int b = a + 1; b < j->n; b++) if (j->obj[a] == j->obj[b]) why = "an object is listed twice (no duplicate orders)";
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
    /* orders: the module reads back the order tree itself (the same facts as the reader), within 3 s, paused-aware */
    char rbx[600] = "";
    if (!strncmp(op, "orders.", 7) && j->posted) {
        const orders_facts *of = &g_t.st2.orders;
        uint64_t vt = of->vt_order + (uint64_t)g_slide;
        long long start = mono_us();
        pending = 1;
        while (pending && mono_us() - start < 3000000) {
            if (!strcmp(op, "orders.delete")) pending = orders_find(of, sread, vt, j->obj[0], j->order_id) != 0;
            else if (!strcmp(op, "orders.execute")) {
                uint8_t ex = 9;
                pending = j->order && !(sread(j->order + of->exec_off, &ex, 1) && ex == (j->flag ? 1 : 0));
                if (!j->order) pending = 0; /* the whole plan: the MCP's read-back judges every order */
            } else if (!strcmp(op, "orders.set_path")) {
                uint64_t pd = 0; int32_t pn = -1, x = 0; int same = 1;
                if (orders_find(of, sread, vt, j->obj[0], j->order_id) != j->order || !sq(j->order + of->provs_off, &pd) ||
                    !sread(j->order + of->provs_cnt, &pn, 4)) same = 0;
                /* executed = the order now holds the provinces the country controls, in the order given: the game keeps
                 * a subsequence of the request (it drops what the country does not control) */
                int k = 0;
                for (int32_t e = 0; same && e < pn; e++) {
                    if (!sread(pd + 4 * (uint64_t)e, &x, 4)) { same = 0; break; }
                    while (k < j->nprov && j->provs[k] != x) k++;
                    if (k == j->nprov) same = 0; else k++;
                }
                pending = !(same && pn > 0);
                if (!pending) snprintf(rbx, sizeof rbx, ",\"provinces_after\":%d", pn);
            } else if (!strncmp(op, "orders2.", 8) && strcmp(op, "orders2.spearhead")) {
                /* engineer 74 */
                const fronts_facts *ff = &g_t.st2.fronts;
                uint64_t g = j->obj[0];
                pending = 1;
                if (!strcmp(op, "orders2.new_root")) {
                    int32_t rid = orders_root_on(of, ff, sread, vt, g, j->hx, (int32_t)j->val[0]);
                    if (rid > 0) { pending = 0; snprintf(rbx, sizeof rbx, ",\"root_id\":%d", rid); }
                } else if (!strcmp(op, "orders2.assign") || !strcmp(op, "orders2.unassign")) {
                    int held = 0, ok_all = 1;
                    uint64_t od = 0; int32_t on = 0;
                    int as = !strcmp(op, "orders2.assign");
                    if (as && (orders_find(of, sread, vt, g, j->order_id) != j->order || !sq(j->order + of->divs_off, &od) ||
                               !sread(j->order + of->divs_cnt, &on, 4))) ok_all = 0;
                    for (int d = 0; ok_all && d < j->ndiv; d++) {
                        if (as) { /* the named order's own list now holds the division */
                            for (int32_t e = 0; e < on && e < 1024; e++) { uint64_t x = 0; if (sq(od + 8 * (uint64_t)e, &x) && x == j->divs[d]) { held++; break; } }
                        } else { /* no order of the group holds it any more */
                            int32_t hid = orders_holder(of, sread, vt, g, j->divs[d]);
                            if (hid < 0) { ok_all = 0; break; }
                            if (hid == 0) held++;
                        }
                    }
                    pending = !(ok_all && held == j->ndiv);
                    if (!pending) snprintf(rbx, sizeof rbx, ",\"divisions_moved\":%d", held);
                } else if (!strcmp(op, "orders2.fallback")) {
                    int32_t cnt = -1;
                    pending = !(sread(g + of->list_cnt[1], &cnt, 4) && cnt > j->before_fallback);
                    if (!pending) snprintf(rbx, sizeof rbx, ",\"fallback_orders\":%d", cnt);
                } else if (!strcmp(op, "orders2.edit_root")) {
                    int64_t rg[2] = {-1, -1};
                    pending = !(orders_find(of, sread, vt, g, j->order_id) == j->order && sread(j->order + ff->root_range_off, rg, 16) &&
                                rg[0] == j->val[1] && rg[1] == j->val[2]);
                    if (!pending) snprintf(rbx, sizeof rbx, ",\"range\":[%lld,%lld]", (long long)rg[0], (long long)rg[1]);
                } else {
                    /* invasion/paradrop target: the module has no read of the target field yet; the order must still be
                     * there and the MCP side reads the plan */
                    pending = orders_find(of, sread, vt, g, j->order_id) != j->order;
                    if (!pending) snprintf(rbx, sizeof rbx, ",\"target_read\":\"not mapped (the order is still there)\"");
                }
            } else if (!strcmp(op, "orders.new_front") || !strcmp(op, "orders2.spearhead")) {
                int32_t cn = -1;
                pending = !(orders_find(of, sread, vt, j->obj[0], j->order_id) == j->order && sread(j->order + of->child_cnt[0], &cn, 4) &&
                            cn > j->before_children);
                if (!pending && j->blitz) {
                    uint64_t cd = 0, sp = 0; uint8_t bz = 0;
                    if (sq(j->order + of->child_off[0], &cd) && sq(cd + 8 * (uint64_t)(cn - 1), &sp)) {
                        /* the spearhead's line is the new child's own child (NewFront: arrow -> line) or the child itself */
                        pending = !(sread(sp + of->blitz_off, &bz, 1) && bz == 1);
                        if (pending) { uint64_t sd = 0, s2 = 0; int32_t sn = 0;
                            if (sq(sp + of->child_off[0], &sd) && sread(sp + of->child_cnt[0], &sn, 4) && sn > 0 && sq(sd, &s2) &&
                                sread(s2 + of->blitz_off, &bz, 1) && bz == 1) pending = 0; }
                    }
                }
                if (!pending) {
                    uint64_t cd = 0; int32_t nid = 0;
                    if (sq(j->order + of->child_off[0], &cd) && sread(cd + 8 * (uint64_t)(cn - 1), &cd, 8) && sread(cd + of->id_off, &nid, 4))
                        snprintf(rbx, sizeof rbx, ",\"new_children\":%d,\"last_child_id\":%d,\"divisions_handed\":%d", cn - j->before_children, nid, j->ndiv);
                }
            }
            if (pending) usleep(2000);
        }
        if (!pending) { executed = j->posted; rb_us = mono_us() - t0; }
        else {
            reads_t r; do_reads(&r);
            if (!(r.ok && r.has_date && r.paused)) snprintf(j->reason, sizeof j->reason, "posted; not seen executed after 3 s in a running game");
            else snprintf(j->reason, sizeof j->reason, "posted; not executed yet (game paused)");
        }
    }
    /* units (engineer 75): the divisions' own army field (the one Assign's check reads): a new army = every division in
     * ONE group that none of them was in before; a transfer = every division in the target army. Within 3 s,
     * paused-aware; the other units ops are judged by the MCP's read-back (the external roster) */
    if ((!strcmp(op, "units.new_army") || !strcmp(op, "units.transfer")) && j->posted) {
        uint32_t go = g_t.st2.common.unit_group_off;
        long long start = mono_us();
        uint64_t g0 = 0;
        pending = 1;
        while (pending && mono_us() - start < 3000000) {
            int same = 1; g0 = 0;
            for (int d = 0; same && d < j->ndiv; d++) {
                uint64_t g = 0;
                if (!sq(j->divs[d] + go, &g) || !g) { same = 0; break; }
                if (!d) g0 = g; else if (g != g0) same = 0;
                if (!strcmp(op, "units.new_army")) { for (int e = 0; same && e < j->ndiv; e++) if (j->grp_before[e] == g) same = 0; }
                else if (g != j->obj[0]) same = 0;
            }
            pending = !same;
            if (pending) usleep(2000);
        }
        if (!pending) { executed = j->posted; rb_us = mono_us() - t0; snprintf(rbx, sizeof rbx, ",\"army\":\"0x%llx\",\"divisions\":%d", (unsigned long long)g0, j->ndiv); }
        else {
            reads_t r; do_reads(&r);
            if (!(r.ok && r.has_date && r.paused)) snprintf(j->reason, sizeof j->reason, "posted; not seen executed after 3 s in a running game");
            else snprintf(j->reason, sizeof j->reason, "posted; not executed yet (game paused)");
        }
    }
    /* holds (engineer 79): every posted state is in the area order (an existing order: its own list; a new order: the
     * one area order of the army that was not there before holds the state, and its id is returned). 3 s, paused-aware */
    if (!strncmp(op, "holds.", 6) && j->posted) {
        const orders_facts *of = &g_t.st2.orders;
        uint64_t vt = of->vt_order + (uint64_t)g_slide, g = j->obj[0];
        long long start = mono_us();
        pending = 1;
        while (pending && mono_us() - start < 3000000) {
            if (j->order_id) {
                int all = orders_find(of, sread, vt, g, j->order_id) == j->order;
                for (int i = 0; all && i < j->n; i++) if (j->res[i] == R_POSTED && hold_has_state(j->order, j->provs[i]) != 1) all = 0;
                pending = !all;
            } else {
                int32_t ids[64]; int n2 = hold_area_ids(g, ids, 64);
                for (int a = 0; a < n2 && pending; a++) {
                    int old = 0;
                    for (int b = 0; b < j->hold_nbefore; b++) if (j->hold_before[b] == ids[a]) old = 1;
                    uint64_t o = old ? 0 : orders_find(of, sread, vt, g, ids[a]);
                    if (o && hold_has_state(o, j->provs[0]) == 1) { pending = 0; j->new_order_id = ids[a]; }
                }
            }
            if (pending) usleep(2000);
        }
        if (!pending) {
            executed = j->posted; rb_us = mono_us() - t0;
            snprintf(rbx, sizeof rbx, ",\"area_order_id\":%d,\"states_added\":%d", j->order_id ? j->order_id : j->new_order_id, j->posted);
        } else {
            reads_t r; do_reads(&r);
            if (!(r.ok && r.has_date && r.paused)) snprintf(j->reason, sizeof j->reason, "posted; the states are not in the area order after 3 s in a running game");
            else snprintf(j->reason, sizeof j->reason, "posted; not executed yet (game paused)");
        }
    }
    /* events (engineer 76): executed = the game state no longer holds the pending record of the window's id with that
     * event (the answer's own "can do" reads the same records). Within 3 s, paused-aware */
    if (!strcmp(op, "events.answer") && j->posted) {
        const events_facts *e = &g_t.st2.events;
        long long start = mono_us();
        pending = 1;
        while (pending && mono_us() - start < 3000000) {
            uint64_t gs = 0, arr = 0; int32_t cnt = 0; int still = 0;
            if (sq(g_gs_glob + (uint64_t)g_slide, &gs) && sread(gs + e->pend_cnt_off, &cnt, 4) && cnt >= 0 && cnt <= 4096 &&
                sq(gs + e->pend_arr_off, &arr)) {
                for (int32_t i = 0; i < cnt && !still; i++) {
                    int32_t id = 0; uint64_t pe = 0, rec = arr + (uint64_t)i * e->pend_size;
                    if (sread(rec, &id, 4) && id == j->ev_id && sq(rec + e->pend_event_off, &pe) && pe == j->ev_event) still = 1;
                }
                pending = still;
            }
            if (pending) usleep(2000);
        }
        if (!pending) { executed = j->posted; rb_us = mono_us() - t0; snprintf(rbx, sizeof rbx, ",\"pending_id\":%d,\"option_index\":%d", j->ev_id, j->ev_option); }
        else {
            reads_t r; do_reads(&r);
            if (!(r.ok && r.has_date && r.paused)) snprintf(j->reason, sizeof j->reason, "posted; the event is still pending after 3 s in a running game");
            else snprintf(j->reason, sizeof j->reason, "posted; not executed yet (game paused)");
        }
    }
    /* mio (engineer 84): the organisation's own fields: the queue holds the trait, the unlocked set holds it, the attached
     * policy is the token sent, each line's organisation handle is the one sent. 3 s, paused-aware */
    if (!strncmp(op, "mio.", 4) && j->posted) {
        const mio_facts *f = &g_t.st2.mio;
        long long start = mono_us();
        pending = 1;
        while (pending && mono_us() - start < 3000000) {
            if (!strcmp(op, "mio.queue_trait")) pending = mio_queued(f, mio_rd_cb, j->obj[0], j->mio_tok) != 1;
            else if (!strcmp(op, "mio.unlock_trait")) pending = mio_unlocked(f, mio_rd_cb, j->obj[0], j->mio_tok) != 1;
            else if (!strcmp(op, "mio.policy")) { int32_t cur = 0; pending = !(sread(j->obj[0] + f->policy_off, &cur, 4) && cur == j->mio_tok); }
            else {
                int all = 1;
                for (int i = 0; i < j->n && all; i++) {
                    uint64_t hh = 0;
                    if (j->res[i] == R_POSTED && !(sq(j->obj[i] + f->line_org_off, &hh) && hh == j->mio_h2)) all = 0;
                }
                pending = !all;
            }
            if (pending) usleep(2000);
        }
        if (!pending) {
            executed = j->posted; rb_us = mono_us() - t0;
            if (!strcmp(op, "mio.policy"))
                snprintf(rbx, sizeof rbx, ",\"policy\":%d,\"policy_before\":%d,\"detached\":%s", j->mio_tok, j->mio_before,
                         j->mio_tok == f->empty_tok ? "true" : "false");
            else if (strcmp(op, "mio.line")) snprintf(rbx, sizeof rbx, ",\"trait\":%d", j->mio_tok);
            else snprintf(rbx, sizeof rbx, ",\"organisation\":\"0x%llx\"", (unsigned long long)j->obj2);
        } else {
            reads_t r; do_reads(&r);
            if (!(r.ok && r.has_date && r.paused)) snprintf(j->reason, sizeof j->reason, "posted; the organisation does not show it after 3 s in a running game");
            else snprintf(j->reason, sizeof j->reason, "posted; not executed yet (game paused)");
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
    if (at < n) at += (size_t)snprintf(out + at, n - at, "]%s", rbx);
    logj("\"event\":\"post2\",\"id\":%ld,%s", id, out);
    int ok = j->ran && !j->unexplained && j->refused == 0 && j->posted == j->n && (pending == 0);
    job2_release(j);
    return ok;
}

/* engineer 73: the order-tree read. A main-thread job (between frames: a consistent snapshot, nothing changes while it
 * reads), on an own group only: the group must round-trip through the game's COrdersGroup resolver and its owner must
 * be the player's tag (the same checks the order commands make). Then a pure read (core/orders.c). */
typedef struct {
    int refs; uint64_t group; int ok, n; char reason[160]; char *out; size_t cap;
    int fronts; /* engineer 74: 1 = the group's theatre fronts instead of its order tree */
    dispatch_semaphore_t done;
} jobo_t;
static void jobo_release(jobo_t *j) {
    if (__atomic_sub_fetch(&j->refs, 1, __ATOMIC_ACQ_REL) == 0) { dispatch_release(j->done); free(j->out); free(j); }
}
static void run_jobo(void *ctx) {
    jobo_t *j = (jobo_t *)ctx;
    const cmd_type *dt = stage2_find(&g_t.st2, "orders.delete");
    uint64_t gs = 0, vp = 0, h = 0; int32_t tag = 0;
    if (killed()) snprintf(j->reason, sizeof j->reason, "kill switch");
    else if (!g_t.st2.orders.ok || !dt || !dt->ok) snprintf(j->reason, sizeof j->reason, "order facts unresolved on this build");
    else if (!g_gs_glob || !sq(g_gs_glob + (uint64_t)g_slide, &gs) || !sq(gs, &vp) || vp != g_vt_gs ||
             !sread(gs + g_t.st2.common.player_tag_off, &tag, 4) || tag <= 0)
        snprintf(j->reason, sizeof j->reason, "no loaded game with a player country");
    else if (!roundtrip(dt, j->group, &h)) snprintf(j->reason, sizeof j->reason, "not a live army or army group");
    else if (!code_ptr(dt->owner_fn + (uint64_t)g_slide) ||
             ((owner_fn2)(uintptr_t)(dt->owner_fn + (uint64_t)g_slide))((void *)(uintptr_t)j->group) != tag)
        snprintf(j->reason, sizeof j->reason, "not an own army or army group");
    else if (j->fronts) {
        const fronts_facts *ff = &g_t.st2.fronts;
        uint64_t th = ff->ok ? fronts_theatre_of(ff, sread, ff->vt_group + (uint64_t)g_slide, ff->vt_theatre + (uint64_t)g_slide, j->group) : 0;
        if (!ff->ok) snprintf(j->reason, sizeof j->reason, "front facts unresolved on this build");
        else if (!th) snprintf(j->reason, sizeof j->reason, "the group has no theatre");
        else {
            j->n = fronts_json(ff, sread, ff->vt_front + (uint64_t)g_slide, ff->vt_section + (uint64_t)g_slide, th, j->out, j->cap);
            j->ok = j->n >= 0;
            if (!j->ok) snprintf(j->reason, sizeof j->reason, "a front is not what the facts say (nothing guessed)");
        }
    } else {
        j->n = orders_tree_json(&g_t.st2.orders, sread, g_t.st2.orders.vt_order + (uint64_t)g_slide, j->group, j->out, j->cap);
        j->ok = j->n >= 0;
        if (!j->ok) snprintf(j->reason, sizeof j->reason, "an order is not what the facts say (nothing guessed)");
    }
    dispatch_semaphore_signal(j->done);
    jobo_release(j);
}
static int be_tree(const char *group, char *out, size_t n, int fronts) {
    uint64_t g[1];
    if (parse_list_u64(group, g, 1) != 1) { snprintf(out, n, "\"reason\":\"group: one hex address\""); return 0; }
    jobo_t *j = (jobo_t *)calloc(1, sizeof *j);
    if (!j || !(j->out = (char *)malloc(n))) { free(j); return 0; }
    j->refs = 2; j->group = g[0]; j->cap = n; j->fronts = fronts; j->done = dispatch_semaphore_create(0);
    long long t0 = mono_us();
    dispatch_async_f(dispatch_get_main_queue(), j, run_jobo);
    if (dispatch_semaphore_wait(j->done, dispatch_time(DISPATCH_TIME_NOW, 1500 * NSEC_PER_MSEC))) {
        snprintf(out, n, "\"reason\":\"the main thread did not run the read within 1.5 s\"");
        jobo_release(j); return 0;
    }
    int ok = j->ok;
    if (ok) { snprintf(out, n, "%s,\"read_ms\":%.2f", j->out, (mono_us() - t0) / 1000.0); }
    else snprintf(out, n, "\"reason\":\"%s\"", j->reason);
    logj("\"event\":\"%s\",\"group\":\"0x%llx\",\"ok\":%s,\"n\":%d,\"ms\":%.2f", fronts ? "fronts_read" : "orders_read",
         (unsigned long long)g[0], ok ? "true" : "false", j->n, (mono_us() - t0) / 1000.0);
    jobo_release(j);
    return ok;
}

/* engineer 84: the organisations read. A main-thread job (a consistent snapshot between frames) over the player's OWN
 * organisation list only; the game's own functions name the tokens and decide "can be unlocked now" and "the policy
 * window allows this item" (core/mio.c). Served whenever the organisation facts resolved, no opt-in: it reads only the
 * player's own country and posts nothing. */
typedef struct { int refs, ok, n; char reason[160]; char *out; size_t cap; dispatch_semaphore_t done; } jobm_t;
static void jobm_release(jobm_t *j) {
    if (__atomic_sub_fetch(&j->refs, 1, __ATOMIC_ACQ_REL) == 0) { dispatch_release(j->done); free(j->out); free(j); }
}
static void run_jobm(void *ctx) {
    jobm_t *j = (jobm_t *)ctx;
    uint64_t gs = 0, vp = 0, mgr = 0; int32_t tag = 0;
    if (killed()) snprintf(j->reason, sizeof j->reason, "kill switch");
    else if (!g_t.st2.mio.ok) snprintf(j->reason, sizeof j->reason, "organisation facts unresolved on this build");
    else if (!g_gs_glob || !sq(g_gs_glob + (uint64_t)g_slide, &gs) || !sq(gs, &vp) || vp != g_vt_gs ||
             !sread(gs + g_t.st2.common.player_tag_off, &tag, 4) || tag <= 0)
        snprintf(j->reason, sizeof j->reason, "no loaded game with a player country");
    else if (!(mgr = mio_manager(tag))) snprintf(j->reason, sizeof j->reason, "the player's organisation manager is unreadable");
    else {
        mio_env e = {mio_rd_cb, mio_name_cb, mio_can_cb, mio_policy_cb, NULL, g_t.st2.mio.vt_org + (uint64_t)g_slide};
        j->n = mio_json(&g_t.st2.mio, &e, mgr, tag, j->out, j->cap);
        j->ok = j->n >= 0;
        if (!j->ok) snprintf(j->reason, sizeof j->reason, "an organisation is not what the facts say (nothing guessed)");
    }
    dispatch_semaphore_signal(j->done);
    jobm_release(j);
}
static int be_mios(char *out, size_t n) {
    jobm_t *j = (jobm_t *)calloc(1, sizeof *j);
    if (!j || !(j->out = (char *)malloc(n))) { free(j); return 0; }
    j->refs = 2; j->cap = n - 64; j->done = dispatch_semaphore_create(0);
    long long t0 = mono_us();
    dispatch_async_f(dispatch_get_main_queue(), j, run_jobm);
    if (dispatch_semaphore_wait(j->done, dispatch_time(DISPATCH_TIME_NOW, 1500 * NSEC_PER_MSEC))) {
        snprintf(out, n, "\"reason\":\"the main thread did not run the read within 1.5 s\"");
        jobm_release(j); return 0;
    }
    int ok = j->ok;
    if (ok) snprintf(out, n, "%s,\"read_ms\":%.2f", j->out, (mono_us() - t0) / 1000.0);
    else snprintf(out, n, "\"reason\":\"%s\"", j->reason);
    logj("\"event\":\"mios_read\",\"ok\":%s,\"n\":%d,\"ms\":%.2f", ok ? "true" : "false", j->n, (mono_us() - t0) / 1000.0);
    jobm_release(j);
    return ok;
}

static int be_orders(const char *group, char *out, size_t n) { return be_tree(group, out, n, 0); }
static int be_fronts(const char *group, char *out, size_t n) { return be_tree(group, out, n, 1); }
static void be_log(const char *members) { logj("%s", members); }

static void *ipc_thread(void *arg) {
    pthread_setname_np("hoi4-live-inproc-ipc");
    static const ipc_backend be = {be_hello, be_read, be_probe, be_post_speed, be_post2, be_orders, be_fronts, be_mios, killed, be_log};
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
