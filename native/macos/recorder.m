// ScreenCaptureKit run-loop bridge. Rust validates and creates the private run;
// this child captures only the pinned owned window and never sends input.
#import <Foundation/Foundation.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <AVFoundation/AVFoundation.h>
#import <ApplicationServices/ApplicationServices.h>
#import <libproc.h>
#import <sys/proc_info.h>
#import <sys/stat.h>
#import <sys/statvfs.h>
#import <errno.h>
#import <unistd.h>
#import <assert.h>

#if __MAC_OS_X_VERSION_MAX_ALLOWED >= 150000
static NSDictionary *session, *owner, *config;
static NSString *directory, *failure;
static CGWindowID windowID;
static CGRect pinnedFrame;
static NSUInteger frames, idleFrames;
static double firstPTS = -1, lastPTS = -1, maxGap, started;
static BOOL finishing, recordingStarted;
static SCStream *stream;
static dispatch_source_t watchdog;
static SCRecordingOutput *recording API_AVAILABLE(macos(15.0));
static NSMutableDictionary *metadata;

// Idle means unchanged pixels, not capture loss. A terminal frame is expected
// only after our stop request. Unknown/missing status never proves visibility.
static BOOL frameStatusAllowed(NSInteger status, BOOL stopping, BOOL hasComplete) {
    switch (status) {
        case SCFrameStatusComplete: case SCFrameStatusIdle: return YES;
        case SCFrameStatusStarted: return !hasComplete || stopping;
        case SCFrameStatusStopped: return stopping;
        default: return stopping;
    }
}
static int testFrameStatuses(void) {
    for (int stopping = 0; stopping < 2; stopping++) {
        for (int hasComplete = 0; hasComplete < 2; hasComplete++) {
            assert(frameStatusAllowed(SCFrameStatusComplete, stopping, hasComplete));
            assert(frameStatusAllowed(SCFrameStatusIdle, stopping, hasComplete));
            assert(frameStatusAllowed(SCFrameStatusStarted, stopping, hasComplete) == (!hasComplete || stopping));
            for (NSNumber *status in @[@(SCFrameStatusBlank), @(SCFrameStatusSuspended), @(SCFrameStatusStopped), @(-1), @999])
                assert(frameStatusAllowed(status.integerValue, stopping, hasComplete) == stopping);
        }
    }
    return 0;
}

static double now(void) { return NSProcessInfo.processInfo.systemUptime; }
static BOOL rejectGuard(NSString *reason, NSDictionary *detail) {
    metadata[@"guard_failure"] = @{@"reason": reason, @"detail": detail,
        @"unix_seconds": @(NSDate.date.timeIntervalSince1970)};
    return NO;
}
static BOOL identity(NSDictionary *expected, pid_t *parent) {
    struct proc_bsdinfo b;
    char path[PROC_PIDPATHINFO_MAXSIZE];
    int pid = [expected[@"pid"] intValue];
    if (pid <= 0 || getuid() == 0) return rejectGuard(@"invalid_identity", @{});
    int n = proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &b, sizeof b);
    if (n != sizeof b) return rejectGuard(@"process_info_read", @{@"pid": @(pid), @"bytes": @(n), @"errno": @(errno)});
    if (proc_pidpath(pid, path, sizeof path) <= 0)
        return rejectGuard(@"process_path_read", @{@"pid": @(pid), @"errno": @(errno)});
    NSString *ticks = [NSString stringWithFormat:@"%llu.%06llu", (unsigned long long)b.pbi_start_tvsec, (unsigned long long)b.pbi_start_tvusec];
    NSString *exe = [NSString stringWithUTF8String:path];
    if (parent) *parent = b.pbi_ppid;
    if (![ticks isEqual:expected[@"start_ticks"]] || ![exe isEqual:expected[@"exe"]]
        || b.pbi_uid != [expected[@"uid"] unsignedIntValue] || b.pbi_uid != getuid())
        return rejectGuard(@"process_identity_changed", @{@"expected": expected, @"actual": @{
            @"pid": @(pid), @"start_ticks": ticks, @"exe": exe ?: @"", @"uid": @(b.pbi_uid), @"parent": @(b.pbi_ppid)}});
    return YES;
}
static BOOL guard(void) {
    pid_t parent = 0;
    if (!identity(owner, NULL) || getppid() != [owner[@"pid"] intValue])
        return rejectGuard(@"recorder_owner_lost", @{@"parent": @(getppid()), @"identity_failure": metadata[@"guard_failure"] ?: @{} });
    if (!identity(session[@"game"], &parent) || !identity(session[@"supervisor"], NULL)) return NO;
    if (parent != [session[@"supervisor"][@"pid"] intValue])
        return rejectGuard(@"game_parent_changed", @{@"actual": @(parent), @"expected": session[@"supervisor"][@"pid"]});
    NSDictionary *login = CFBridgingRelease(CGSessionCopyCurrentDictionary());
    BOOL console = [login[(__bridge NSString *)kCGSessionOnConsoleKey] boolValue];
    BOOL done = [login[(__bridge NSString *)kCGSessionLoginDoneKey] boolValue];
    BOOL locked = [login[@"CGSSessionScreenIsLocked"] boolValue];
    NSNumber *uid = login[(__bridge NSString *)kCGSessionUserIDKey];
    if (!console || !done || locked || !uid || uid.unsignedIntValue != getuid())
        return rejectGuard(@"desktop_session", @{@"dictionary_present": @(login != nil), @"on_console": @(console),
            @"login_done": @(done), @"locked": @(locked), @"uid": uid ?: [NSNull null]});
    NSArray *items = CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionIncludingWindow, windowID));
    for (NSDictionary *item in items) {
        if ([item[(__bridge NSString *)kCGWindowNumber] unsignedIntValue] != windowID) continue;
        CGRect frame = CGRectZero;
        BOOL valid = CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)item[(__bridge NSString *)kCGWindowBounds], &frame);
        BOOL owned = [item[(__bridge NSString *)kCGWindowOwnerPID] intValue] == [session[@"game"][@"pid"] intValue];
        BOOL shown = [item[(__bridge NSString *)kCGWindowIsOnscreen] boolValue];
        BOOL layer = [item[(__bridge NSString *)kCGWindowLayer] intValue] == 0;
        if (!owned || !shown || !valid || !layer || !CGRectEqualToRect(frame, pinnedFrame))
            return rejectGuard(@"window_changed", @{@"window_id": @(windowID), @"owner": item[(__bridge NSString *)kCGWindowOwnerPID] ?: [NSNull null],
                @"owned": @(owned), @"onscreen": @(shown), @"frame_valid": @(valid), @"normal_layer": @(layer),
                @"frame": NSStringFromRect(frame), @"pinned_frame": NSStringFromRect(pinnedFrame)});
        return YES;
    }
    return rejectGuard(@"window_missing", @{@"window_id": @(windowID), @"list_present": @(items != nil), @"list_count": @(items.count)});
}
static void save(NSString *status) {
    metadata[@"status"] = status;
    metadata[@"frames_complete"] = @(frames); metadata[@"frames_idle"] = @(idleFrames);
    metadata[@"first_pts"] = @(firstPTS); metadata[@"last_pts"] = @(lastPTS);
    metadata[@"max_complete_frame_gap_seconds"] = @(maxGap);
    metadata[@"elapsed_seconds"] = @(now() - started);
    metadata[@"updated_unix_seconds"] = @(NSDate.date.timeIntervalSince1970);
    metadata[@"recording_started"] = @(recordingStarted);
    if (failure) metadata[@"failure"] = failure;
    NSError *error = nil;
    NSData *data = [NSJSONSerialization dataWithJSONObject:metadata options:NSJSONWritingPrettyPrinted error:&error];
    if (!data || ![data writeToFile:[directory stringByAppendingPathComponent:@"result.json"] options:NSDataWritingAtomic error:&error]) {
        fprintf(stderr, "Cannot persist recorder status: %s\n", error.description.UTF8String);
        exit(1);
    }
}
static void stop(NSString *reason) API_AVAILABLE(macos(15.0));
static void stop(NSString *reason) {
    if (finishing) return;
    finishing = YES;
    metadata[@"stop_reason"] = reason; save(@"stopping");
    if (!stream) { failure = failure ?: @"capture never started"; save(@"error"); exit(1); }
    [stream stopCaptureWithCompletionHandler:^(NSError *error) {
        dispatch_async(dispatch_get_main_queue(), ^{
            if (error) { failure = error.description; save(@"error"); exit(1); }
        });
    }];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 10 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
        metadata[@"finalization_failure"] = @"recording finalization deadline";
        failure = failure ?: @"recording finalization deadline"; save(@"error"); exit(1);
    });
}
API_AVAILABLE(macos(15.0))
@interface Observer : NSObject<SCStreamOutput, SCStreamDelegate, SCRecordingOutputDelegate>
@end
@implementation Observer
- (void)stream:(SCStream *)sender didOutputSampleBuffer:(CMSampleBufferRef)sample ofType:(SCStreamOutputType)type {
    if (type != SCStreamOutputTypeScreen) return;
    BOOL valid = CMSampleBufferIsValid(sample);
    NSArray *attachments = valid ? (__bridge NSArray *)CMSampleBufferGetSampleAttachmentsArray(sample, false) : nil;
    NSNumber *status = attachments.firstObject[SCStreamFrameInfoStatus];
    NSInteger value = status ? status.integerValue : -1;
    NSMutableDictionary *counts = metadata[@"frame_status_counts"];
    NSString *key = [NSString stringWithFormat:@"%ld", (long)value];
    counts[key] = @([counts[key] unsignedIntegerValue] + 1);
    if (!frameStatusAllowed(value, finishing, frames > 0)) {
        metadata[@"frame_failure"] = @{@"status": @(value), @"valid_sample": @(valid), @"frames_before_loss": @(frames)};
        failure = @"unexpected capture frame status"; stop(@"frame_failure"); return;
    }
    if (value == SCFrameStatusIdle) idleFrames++;
    if (value != SCFrameStatusComplete) return;
    double pts = CMTimeGetSeconds(CMSampleBufferGetPresentationTimeStamp(sample));
    if (!isfinite(pts)) { failure = @"invalid complete frame timestamp"; stop(@"frame_failure"); return; }
    if (firstPTS < 0) firstPTS = pts;
    if (lastPTS >= 0) maxGap = fmax(maxGap, pts - lastPTS);
    lastPTS = pts; frames++;
}
- (void)stream:(SCStream *)sender didStopWithError:(NSError *)error {
    dispatch_async(dispatch_get_main_queue(), ^{ failure = error.description; save(@"error"); exit(1); });
}
- (void)recordingOutputDidStartRecording:(SCRecordingOutput *)output {
    dispatch_async(dispatch_get_main_queue(), ^{ recordingStarted = YES; save(finishing ? @"stopping" : @"recording"); });
}
- (void)recordingOutput:(SCRecordingOutput *)output didFailWithError:(NSError *)error {
    dispatch_async(dispatch_get_main_queue(), ^{ failure = error.description; save(@"error"); exit(1); });
}
- (void)recordingOutputDidFinishRecording:(SCRecordingOutput *)output {
    dispatch_async(dispatch_get_main_queue(), ^{
        double seconds = CMTimeGetSeconds(output.recordedDuration);
        metadata[@"recorded_seconds"] = @(isfinite(seconds) ? seconds : 0);
        metadata[@"recorded_bytes"] = @(output.recordedFileSize);
        if (!finishing || !recordingStarted || frames == 0 || !isfinite(seconds) || seconds <= 0)
            failure = failure ?: @"recording ended without a complete requested capture";
        save(failure ? @"error" : @"complete"); exit(failure ? 1 : 0);
    });
}
@end

// ScreenCaptureKit delegates/outputs do not own this callback target. Keep it
// alive through the last recording callback, after capture() has returned.
static Observer *observer API_AVAILABLE(macos(15.0));
static void capture(void) API_AVAILABLE(macos(15.0));
static void capture(void) {
    observer = [Observer new];
    if (!guard()) { failure = @"initial ownership/window/session guard"; save(@"error"); exit(1); }
    if (!CGPreflightScreenCaptureAccess()) { failure = @"screen recording permission unavailable"; save(@"error"); exit(1); }
    watchdog = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
    dispatch_source_set_timer(watchdog, dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC / 4), NSEC_PER_SEC / 4, NSEC_PER_SEC / 20);
    dispatch_source_set_event_handler(watchdog, ^{
        if (finishing) return;
        if (!guard()) { failure = @"ownership/window/session guard"; stop(@"guard_failure"); return; }
        struct statvfs fs;
        if (statvfs(directory.fileSystemRepresentation, &fs) != 0
            || (long double)fs.f_bavail * fs.f_frsize < [config[@"min_free_bytes"] unsignedLongLongValue]
            || (unsigned long long)recording.recordedFileSize >= [config[@"max_bytes"] unsignedLongLongValue]) {
            failure = @"storage budget reached or unavailable"; stop(@"storage_budget"); return;
        }
        if (!recordingStarted && now() - started >= 10) { failure = @"recording startup deadline"; stop(@"startup_deadline"); return; }
        if ([[NSFileManager defaultManager] fileExistsAtPath:[directory stringByAppendingPathComponent:@"stop.request"]]) { stop(@"explicit_stop"); return; }
        if (now() - started >= [config[@"max_seconds"] doubleValue]) { stop(@"duration"); return; }
        save(recordingStarted ? @"recording" : @"starting");
    });
    dispatch_resume(watchdog);
    [SCShareableContent getShareableContentExcludingDesktopWindows:YES onScreenWindowsOnly:YES completionHandler:^(SCShareableContent *content, NSError *error) {
        dispatch_async(dispatch_get_main_queue(), ^{
            if (finishing) return;
            if (error) { failure = error.description; save(@"error"); exit(1); }
            SCWindow *chosen = nil;
            for (SCWindow *w in content.windows)
                if (w.windowID == windowID && w.owningApplication.processID == [session[@"game"][@"pid"] intValue]) chosen = w;
            if (!chosen || !CGRectEqualToRect(chosen.frame, pinnedFrame) || !guard()) {
                failure = @"shareable owned window or pinned geometry mismatch"; save(@"error"); exit(1);
            }
            SCContentFilter *filter = [[SCContentFilter alloc] initWithDesktopIndependentWindow:chosen];
            metadata[@"source_frame"] = NSStringFromRect(pinnedFrame);
            metadata[@"content_rect"] = NSStringFromRect(filter.contentRect);
            metadata[@"backing_scale"] = @(filter.pointPixelScale);
            SCStreamConfiguration *sc = [SCStreamConfiguration new];
            sc.width = pinnedFrame.size.width; sc.height = pinnedFrame.size.height;
            sc.minimumFrameInterval = CMTimeMake(1, [config[@"fps"] intValue]); sc.queueDepth = 5;
            sc.capturesAudio = NO; sc.showsCursor = YES; sc.ignoreShadowsSingleWindow = YES; sc.scalesToFit = YES;
            SCRecordingOutputConfiguration *rc = [SCRecordingOutputConfiguration new];
            rc.outputURL = [NSURL fileURLWithPath:[directory stringByAppendingPathComponent:@"video.mp4"]];
            rc.videoCodecType = AVVideoCodecTypeH264; rc.outputFileType = AVFileTypeMPEG4;
            recording = [[SCRecordingOutput alloc] initWithConfiguration:rc delegate:observer];
            stream = [[SCStream alloc] initWithFilter:filter configuration:sc delegate:observer];
            NSError *e = nil;
            if (![stream addStreamOutput:observer type:SCStreamOutputTypeScreen sampleHandlerQueue:dispatch_get_main_queue() error:&e]
                || ![stream addRecordingOutput:recording error:&e]) {
                failure = e.description; save(@"error"); exit(1);
            }
            [stream startCaptureWithCompletionHandler:^(NSError *e) {
                dispatch_async(dispatch_get_main_queue(), ^{ if (e) { failure = e.description; save(@"error"); exit(1); } });
            }];
        });
    }];
}
#endif

int main(int argc, char **argv) { @autoreleasepool {
    umask(0077);
#if __MAC_OS_X_VERSION_MAX_ALLOWED >= 150000
    if (argc == 2 && strcmp(argv[1], "--self-test-frame-status") == 0) return testFrameStatuses();
    if (argc != 2) return 2;
    NSData *data = [NSData dataWithContentsOfFile:@(argv[1]) options:NSDataReadingMappedIfSafe error:nil];
    if (!data || data.length > 65536) return 2;
    NSDictionary *request = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
    session = request[@"session"]; owner = request[@"recorder_owner"]; config = request[@"config"];
    if (![session isKindOfClass:NSDictionary.class] || ![owner isKindOfClass:NSDictionary.class]
        || ![config isKindOfClass:NSDictionary.class]) return 2;
    directory = config[@"output"]; windowID = [config[@"window_id"] unsignedIntValue];
    NSDictionary *frame = config[@"frame"];
    pinnedFrame = CGRectMake([frame[@"x"] intValue], [frame[@"y"] intValue], [frame[@"width"] unsignedIntValue], [frame[@"height"] unsignedIntValue]);
    struct stat st;
    if (![directory isKindOfClass:NSString.class] || lstat(directory.fileSystemRepresentation, &st) != 0
        || !S_ISDIR(st.st_mode) || st.st_uid != getuid() || (st.st_mode & 0077) != 0
        || [[NSFileManager defaultManager] fileExistsAtPath:[directory stringByAppendingPathComponent:@"video.mp4"]]) return 2;
    started = now();
    metadata = [@{@"owned_session": session, @"recorder_owner": owner, @"window_id": @(windowID),
        @"capture": @"desktop-independent-window", @"requested_fps": config[@"fps"],
        @"duration_limit": config[@"max_seconds"], @"client_bounds_verified": @NO} mutableCopy];
    metadata[@"frame_status_counts"] = [NSMutableDictionary new];
    save(@"starting");
    if (@available(macOS 15.0, *)) { capture(); dispatch_main(); }
    failure = @"ScreenCaptureKit recording requires macOS 15 or later"; save(@"error");
#else
    fprintf(stderr, "ScreenCaptureKit recorder requires compilation with macOS SDK 15 or later\n");
#endif
    return 1;
} }
