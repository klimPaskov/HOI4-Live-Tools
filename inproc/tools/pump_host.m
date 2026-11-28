/* pump_host: does dispatch_async_f onto the main queue run inside a frame loop that pumps events the way SDL2's
 * Cocoa backend does (nextEventMatchingMask:untilDate:distantPast inMode:NSDefaultRunLoopMode dequeue:YES, once per
 * frame, never CFRunLoopRun)? A background thread queues 200 jobs; each reports whether it ran on the main thread and
 * how long it waited. Offline stand-in for the game's main loop; touches no game. Prints one JSON line. */
#import <AppKit/AppKit.h>
#include <dispatch/dispatch.h>
#include <pthread.h>
#include <time.h>
static long long us(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec * 1000000LL + t.tv_nsec / 1000; }
static volatile int ran = 0, on_main = 0; static long long waits[200];
typedef struct { long long q; int i; } J;
static void job(void *c) { J *j = c; waits[j->i] = us() - j->q; on_main += pthread_main_np() == 1; ran++; free(j); }
static void *producer(void *a) {
    (void)a; usleep(200000);
    for (int i = 0; i < 200; i++) { J *j = malloc(sizeof *j); j->q = us(); j->i = i; dispatch_async_f(dispatch_get_main_queue(), j, job); usleep(5000); }
    return NULL;
}
int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        pthread_t t; pthread_create(&t, NULL, producer, NULL);
        long long end = us() + 4000000;
        while (us() < end && ran < 200) {
            NSEvent *e;
            while ((e = [NSApp nextEventMatchingMask:NSEventMaskAny untilDate:[NSDate distantPast] inMode:NSDefaultRunLoopMode dequeue:YES]))
                [NSApp sendEvent:e];
            usleep(16667); /* the rest of a 60 fps frame: simulation, UI, render */
        }
        long long mx = 0, sum = 0; for (int i = 0; i < ran; i++) { sum += waits[i]; if (waits[i] > mx) mx = waits[i]; }
        printf("{\"queued\":200,\"ran\":%d,\"on_main\":%d,\"mean_wait_ms\":%.2f,\"max_wait_ms\":%.2f}\n", ran, on_main, ran ? sum / 1000.0 / ran : -1, mx / 1000.0);
        return ran == 200 && on_main == 200 ? 0 : 1;
    }
}
