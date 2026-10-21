#import <Cocoa/Cocoa.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>

char* macos_get_display_name(const char* file_path) {
    @autoreleasepool {
        if (!file_path) return NULL;
        NSString* path = [NSString stringWithUTF8String:file_path];
        if (!path) return NULL;
        NSString* name = [[NSFileManager defaultManager] displayNameAtPath:path];
        if (!name || [name length] == 0) {
            name = [[path lastPathComponent] stringByDeletingPathExtension];
        }
        const char* utf8 = [name UTF8String];
        if (!utf8) return NULL;
        return strdup(utf8);
    }
}

unsigned char* macos_get_icon_png(const char* file_path, int target_size, int* out_len) {
    @autoreleasepool {
        if (!file_path || !out_len) return NULL;
        *out_len = 0;
        NSString* path = [NSString stringWithUTF8String:file_path];
        if (!path) return NULL;
        
        NSWorkspace* ws = [NSWorkspace sharedWorkspace];
        NSImage* icon = [ws iconForFile:path];
        if (!icon) return NULL;
        
        NSSize size = NSMakeSize(target_size, target_size);
        NSImage* resized = [[NSImage alloc] initWithSize:size];
        [resized lockFocus];
        [[NSGraphicsContext currentContext] setImageInterpolation:NSImageInterpolationHigh];
        [icon drawInRect:NSMakeRect(0, 0, target_size, target_size)
                fromRect:NSZeroRect
               operation:NSCompositingOperationCopy
                fraction:1.0];
        [resized unlockFocus];
        
        NSData* tiff = [resized TIFFRepresentation];
        if (!tiff) return NULL;
        NSBitmapImageRep* rep = [NSBitmapImageRep imageRepWithData:tiff];
        if (!rep) return NULL;
        NSData* png = [rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
        if (!png) return NULL;
        
        int len = (int)[png length];
        *out_len = len;
        unsigned char* buf = (unsigned char*)malloc(len);
        if (!buf) return NULL;
        memcpy(buf, [png bytes], len);
        return buf;
    }
}

char* macos_pick_app_file(void) {
    @autoreleasepool {
        NSOpenPanel* panel = [NSOpenPanel openPanel];
        [panel setCanChooseFiles:YES];
        [panel setCanChooseDirectories:YES];
        [panel setAllowsMultipleSelection:NO];
        [panel setDirectoryURL:[NSURL fileURLWithPath:@"/Applications"]];
        [panel setMessage:@"请选择要添加到应用盒的应用程序"];
        [panel setPrompt:@"添加到应用盒"];
        
        if (@available(macOS 11.0, *)) {
            UTType* appType = [UTType typeWithIdentifier:@"com.apple.application-bundle"];
            if (appType) {
                [panel setAllowedContentTypes:@[appType]];
            }
        }
        
        [panel setLevel:NSPopUpMenuWindowLevel];
        NSInteger result = [panel runModal];
        if (result == NSModalResponseOK) {
            NSURL* url = [[panel URLs] firstObject];
            if (url) {
                return strdup([[url path] UTF8String]);
            }
        }
        return NULL;
    }
}
