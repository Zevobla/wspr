// Objective-C shim over Apple's on-device speech recognizer (Speech.framework).
//
// whspr's Rust `AppleSpeech` backend (apple_speech.rs) calls the one C entry
// point below from a `spawn_blocking` worker; the framework's async callbacks
// arrive on internal queues, so we drive them to completion with a semaphore
// and hand back a plain UTF-8 C string. Nothing here touches the network:
// `requiresOnDeviceRecognition` is set whenever the locale's on-device model
// is present, so audio never leaves the machine.
//
// Deliberately compiled WITHOUT ARC (manual reference counting) and with
// `-fno-exceptions` (see build.rs), and it captures only plain scalars in its
// blocks (a pointer to the stack `shim_result`, never `__block` objects).
// ARC cleanups, `@autoreleasepool`, exceptions, and `__block` object captures
// each make clang emit an `___objc_personality_v0` unwind personality; this
// binary already links the C++ ML backends (whisper/llama/sherpa), and arm64's
// compact-unwind format can't encode a 4th distinct personality ("too many
// personality routines for compact unwind"). Keeping this object
// personality-free preserves compact unwind — and Rust's own panic unwinding.

#import <Foundation/Foundation.h>
#import <Speech/Speech.h>
#import <AVFoundation/AVFoundation.h>
#include <stdlib.h>
#include <string.h>

// Callback scratch space, lives on `whspr_apple_transcribe`'s stack. The blocks
// capture a pointer to it (a scalar) and write results in, so there are no
// `__block` object variables to manage. `strdup`ing inside the callback (while
// the framework's autoreleased NSString is still live) means we never hold an
// Objective-C object past the callback.
typedef struct {
    int auth;       // SFSpeechRecognizerAuthorizationStatus
    char *text;     // malloc'd transcript, or NULL
    char *err;      // malloc'd error message, or NULL
} shim_result;

static char *whspr_dup_utf8(NSString *s) {
    if (!s) {
        return NULL;
    }
    const char *utf8 = [s UTF8String];
    return utf8 ? strdup(utf8) : NULL;
}

// Transcribe `n_samples` of mono float32 PCM at `sample_rate` Hz with Apple's
// speech recognizer. `locale_id` is a BCP-47 identifier (e.g. "en-US") or NULL
// for the system default. Blocks up to `timeout_secs` seconds.
//
// On success returns a malloc'd UTF-8 transcript (free with
// whspr_apple_string_free). On failure returns NULL and, if `err_out` is
// non-NULL, writes a malloc'd error message to *err_out (also caller-freed).
extern "C" char *whspr_apple_transcribe(const float *samples,
                                        size_t n_samples,
                                        double sample_rate,
                                        const char *locale_id,
                                        double timeout_secs,
                                        char **err_out) {
    NSAutoreleasePool *pool = [[NSAutoreleasePool alloc] init];
    if (err_out) {
        *err_out = NULL;
    }
    shim_result res = {SFSpeechRecognizerAuthorizationStatusNotDetermined, NULL, NULL};
    shim_result *rp = &res;

    // 1) Authorization is a one-time, async system prompt; wait for it.
    dispatch_semaphore_t auth_sem = dispatch_semaphore_create(0);
    [SFSpeechRecognizer requestAuthorization:^(SFSpeechRecognizerAuthorizationStatus status) {
        rp->auth = (int)status;
        dispatch_semaphore_signal(auth_sem);
    }];
    dispatch_semaphore_wait(auth_sem, DISPATCH_TIME_FOREVER);
    dispatch_release(auth_sem);
    if (res.auth != SFSpeechRecognizerAuthorizationStatusAuthorized) {
        if (err_out) {
            *err_out = strdup("speech recognition not authorized — grant access in "
                              "System Settings > Privacy & Security > Speech Recognition");
        }
        [pool release];
        return NULL;
    }

    // 2) Recognizer for the requested locale (or the system default).
    SFSpeechRecognizer *rec = nil;
    if (locale_id) {
        NSLocale *loc =
            [NSLocale localeWithLocaleIdentifier:[NSString stringWithUTF8String:locale_id]];
        rec = [[SFSpeechRecognizer alloc] initWithLocale:loc];
    } else {
        rec = [[SFSpeechRecognizer alloc] init];
    }
    if (!rec || !rec.isAvailable) {
        if (err_out) {
            *err_out = strdup("speech recognizer unavailable for the requested locale");
        }
        [rec release];
        [pool release];
        return NULL;
    }

    // 3) Wrap the caller's PCM in a mono float32 AVAudioPCMBuffer.
    AVAudioFormat *fmt = [[AVAudioFormat alloc] initWithCommonFormat:AVAudioPCMFormatFloat32
                                                         sampleRate:sample_rate
                                                           channels:1
                                                        interleaved:NO];
    AVAudioPCMBuffer *buf = nil;
    if (fmt) {
        buf = [[AVAudioPCMBuffer alloc] initWithPCMFormat:fmt
                                            frameCapacity:(AVAudioFrameCount)n_samples];
    }
    if (!buf) {
        if (err_out) {
            *err_out = strdup("failed to build audio buffer for speech recognition");
        }
        [buf release];
        [fmt release];
        [rec release];
        [pool release];
        return NULL;
    }
    buf.frameLength = (AVAudioFrameCount)n_samples;
    if (n_samples > 0) {
        memcpy(buf.floatChannelData[0], samples, n_samples * sizeof(float));
    }

    // 4) Recognition request — final result only, on-device when we can.
    SFSpeechAudioBufferRecognitionRequest *req =
        [[SFSpeechAudioBufferRecognitionRequest alloc] init];
    req.shouldReportPartialResults = NO;
    req.requiresOnDeviceRecognition = rec.supportsOnDeviceRecognition;
    if (@available(macOS 13.0, *)) {
        req.addsPunctuation = YES;
    }

    dispatch_semaphore_t done = dispatch_semaphore_create(0);
    SFSpeechRecognitionTask *task = [rec
        recognitionTaskWithRequest:req
                     resultHandler:^(SFSpeechRecognitionResult *result, NSError *error) {
                         if (error) {
                             rp->err = whspr_dup_utf8(error.localizedDescription);
                             dispatch_semaphore_signal(done);
                             return;
                         }
                         if (result && result.isFinal) {
                             rp->text = whspr_dup_utf8(result.bestTranscription.formattedString);
                             dispatch_semaphore_signal(done);
                         }
                     }];

    [req appendAudioPCMBuffer:buf];
    [req endAudio];

    dispatch_time_t deadline =
        dispatch_time(DISPATCH_TIME_NOW, (int64_t)(timeout_secs * NSEC_PER_SEC));
    long timed_out = dispatch_semaphore_wait(done, deadline);
    [task cancel];
    dispatch_release(done);

    char *ret = NULL;
    if (timed_out != 0) {
        free(res.text);
        free(res.err);
        if (err_out) {
            *err_out = strdup("speech recognition timed out");
        }
    } else if (res.text) {
        ret = res.text;
        free(res.err);
    } else {
        if (err_out) {
            *err_out = res.err ? res.err : strdup("speech recognition produced no result");
        } else {
            free(res.err);
        }
    }

    [req release];
    [buf release];
    [fmt release];
    [rec release];
    [pool release];
    return ret;
}

// Free a string returned by whspr_apple_transcribe (result or *err_out).
extern "C" void whspr_apple_string_free(char *s) {
    if (s) {
        free(s);
    }
}
