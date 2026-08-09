#import <Foundation/Foundation.h>
#import <UIKit/UIKit.h>
#import "wml2viewer_ios.h"

@interface WML2ViewerApplicationDelegate : UIResponder <UIApplicationDelegate>
@end

@implementation WML2ViewerApplicationDelegate

- (BOOL)application:(UIApplication *)application
    didFinishLaunchingWithOptions:(NSDictionary<UIApplicationLaunchOptionsKey, id> *)launchOptions {
    (void)application;
    NSURL *url = launchOptions[UIApplicationLaunchOptionsURLKey];
    if (url.isFileURL) {
        wml2viewer_ios_receive_external_path(url.fileSystemRepresentation);
    }
    return YES;
}

- (BOOL)application:(UIApplication *)application
    openURL:(NSURL *)url
    options:(NSDictionary<UIApplicationOpenURLOptionsKey, id> *)options {
    (void)application;
    (void)options;
    if (!url.isFileURL) {
        return NO;
    }
    wml2viewer_ios_receive_external_path(url.fileSystemRepresentation);
    return YES;
}

@end

@interface WML2ViewerApplication : UIApplication
@end

@implementation WML2ViewerApplication

- (instancetype)init {
    self = [super init];
    if (self != nil) {
        self.delegate = [[WML2ViewerApplicationDelegate alloc] init];
    }
    return self;
}

@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        wml2viewer_ios_initialize_bridge();
        NSFileManager *fileManager = [NSFileManager defaultManager];
        NSURL *appSupport = [fileManager URLForDirectory:NSApplicationSupportDirectory
                                                 inDomain:NSUserDomainMask
                                        appropriateForURL:nil
                                                   create:YES
                                                    error:nil];
        NSURL *documents = [fileManager URLsForDirectory:NSDocumentDirectory
                                                inDomains:NSUserDomainMask].firstObject;
        NSURL *caches = [fileManager URLForDirectory:NSCachesDirectory
                                             inDomain:NSUserDomainMask
                                    appropriateForURL:nil
                                               create:YES
                                                error:nil];
        return (int)wml2viewer_ios_main(appSupport.fileSystemRepresentation,
                                         documents.fileSystemRepresentation,
                                         caches.fileSystemRepresentation);
    }
}
