#import <Foundation/Foundation.h>
#import <Speech/Speech.h>
#import <AVFoundation/AVFoundation.h>

int main(int argc, const char * argv[]) {
    @autoreleasepool {
        if (argc < 2) {
            fprintf(stderr, "Usage: apple-speech-cli <audio.wav>\n");
            return 1;
        }

        NSString *filePath = [NSString stringWithUTF8String:argv[1]];
        NSURL *fileURL = [NSURL fileURLWithPath:filePath];

        NSLocale *locale = [NSLocale localeWithLocaleIdentifier:@"en-US"];
        SFSpeechRecognizer *recognizer = [[SFSpeechRecognizer alloc] initWithLocale:locale];
        if (!recognizer || !recognizer.isAvailable) {
            fprintf(stderr, "Apple Speech Recognizer not available\n");
            return 1;
        }

        SFSpeechURLRecognitionRequest *request = [[SFSpeechURLRecognitionRequest alloc] initWithURL:fileURL];
        request.shouldReportPartialResults = NO;
        if (@available(macOS 10.15, *)) {
            if (recognizer.supportsOnDeviceRecognition) {
                request.requiresOnDeviceRecognition = YES;
            }
        }

        dispatch_semaphore_t sema = dispatch_semaphore_create(0);
        __block NSString *transcription = @"";
        __block int exitCode = 0;

        [recognizer recognitionTaskWithRequest:request resultHandler:^(SFSpeechRecognitionResult * _Nullable result, NSError * _Nullable error) {
            if (error) {
                fprintf(stderr, "Speech recognition error: %s\n", error.localizedDescription.UTF8String);
                exitCode = 1;
                dispatch_semaphore_signal(sema);
                return;
            }

            if (result) {
                if (result.isFinal) {
                    transcription = result.bestTranscription.formattedString;
                    dispatch_semaphore_signal(sema);
                }
            }
        }];

        // Wait up to 10 seconds for transcription
        dispatch_time_t timeout = dispatch_time(DISPATCH_TIME_NOW, 10 * NSEC_PER_SEC);
        if (dispatch_semaphore_wait(sema, timeout) != 0) {
            fprintf(stderr, "Speech recognition timed out\n");
            return 1;
        }

        if (exitCode == 0 && transcription.length > 0) {
            printf("%s\n", transcription.UTF8String);
        }
    }
    return 0;
}
