#import <Cocoa/Cocoa.h>
#import <QuartzCore/QuartzCore.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#import <objc/runtime.h>
#import <objc/message.h>

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

// =============================================================================
// macOS Native Liquid Glass Engine (NSGlassEffectView + SDF Optical Lensing)
// =============================================================================

static const void *kGotifyGlassActiveKey  = &kGotifyGlassActiveKey;
static const void *kGotifyGlassOpacityKey = &kGotifyGlassOpacityKey;
static const void *kGotifyGlassDarkKey    = &kGotifyGlassDarkKey;
static const void *kGotifyGlassVariantKey = &kGotifyGlassVariantKey;
static const void *kGotifyGlassCornerKey  = &kGotifyGlassCornerKey;
static const void *kGotifyGlassInsetKey   = &kGotifyGlassInsetKey;

static CALayer* gotify_find_backdrop_layer(CALayer *layer) {
    if (!layer) return nil;
    if ([NSStringFromClass([layer class]) containsString:@"CABackdropLayer"]) {
        return layer;
    }
    for (CALayer *sub in layer.sublayers) {
        CALayer *found = gotify_find_backdrop_layer(sub);
        if (found) return found;
    }
    return nil;
}

static void gotify_make_webviews_transparent(NSView *view) {
    if (!view) return;
    NSString *clsName = NSStringFromClass([view class]);
    if ([clsName containsString:@"WKWebView"] || [clsName containsString:@"WryWebView"]) {
        @try {
            [view setValue:@(NO) forKey:@"drawsBackground"];
        } @catch (NSException * _) {}
        SEL underSel = NSSelectorFromString(@"setUnderPageBackgroundColor:");
        if ([view respondsToSelector:underSel]) {
            ((void (*)(id, SEL, id))objc_msgSend)(view, underSel, [NSColor clearColor]);
        }
    }
    for (NSView *sub in view.subviews) {
        gotify_make_webviews_transparent(sub);
    }
}

static const void *kGotifyGlassRefractKey   = &kGotifyGlassRefractKey;
static const void *kGotifyGlassBlurKey      = &kGotifyGlassBlurKey;
static const void *kGotifyGlassHighKey      = &kGotifyGlassHighKey;
static const void *kGotifyGlassArcHeightKey = &kGotifyGlassArcHeightKey;

static void gotify_traverse_glass_layers(CALayer *layer, double highlightMul) {
    if (!layer) return;
    if ([layer.name isEqualToString:@"@2"] || [layer.name isEqualToString:@"@3"]) {
        layer.opacity = (float)fmin(1.0, 0.75 * highlightMul);
    }
    for (CALayer *ch in layer.sublayers) {
        gotify_traverse_glass_layers(ch, highlightMul);
    }
}

// 递归清除玻璃视图图层树上的所有 CoreAnimation 动画：
// Apple 在窗口失焦时会给高光/滤镜层挂过渡动画，仅改模型值无法阻止闪烁，
// 必须把动画移除才能在同一帧直接呈现用户设定值。
static void gotify_kill_layer_animations(CALayer *layer) {
    if (!layer) return;
    [layer removeAllAnimations];
    for (CALayer *ch in layer.sublayers) {
        gotify_kill_layer_animations(ch);
    }
}

static BOOL gotify_applying_tuning = NO;

static void gotify_apply_liquid_glass_tuning_impl(NSView *glassView, BOOL flush) {
    if (!glassView) return;
    // 防重入：调优过程中会调用 set_subduedState:，该方法的 swizzle 又会回调本函数，
    // 不加以保护会无限递归导致启动时栈溢出。
    if (gotify_applying_tuning) return;
    gotify_applying_tuning = YES;

    NSWindow *win = glassView.window;

    NSNumber *darkNum       = objc_getAssociatedObject(glassView, kGotifyGlassDarkKey);
    if (!darkNum && win)    darkNum = objc_getAssociatedObject(win, kGotifyGlassDarkKey);

    NSNumber *variantNum    = objc_getAssociatedObject(glassView, kGotifyGlassVariantKey);
    if (!variantNum && win) variantNum = objc_getAssociatedObject(win, kGotifyGlassVariantKey);

    NSNumber *refractNum    = objc_getAssociatedObject(glassView, kGotifyGlassRefractKey);
    if (!refractNum && win) refractNum = objc_getAssociatedObject(win, kGotifyGlassRefractKey);

    NSNumber *blurNum       = objc_getAssociatedObject(glassView, kGotifyGlassBlurKey);
    if (!blurNum && win)    blurNum = objc_getAssociatedObject(win, kGotifyGlassBlurKey);

    NSNumber *highNum       = objc_getAssociatedObject(glassView, kGotifyGlassHighKey);
    if (!highNum && win)    highNum = objc_getAssociatedObject(win, kGotifyGlassHighKey);

    NSNumber *arcHeightNum  = objc_getAssociatedObject(glassView, kGotifyGlassArcHeightKey);
    if (!arcHeightNum && win) arcHeightNum = objc_getAssociatedObject(win, kGotifyGlassArcHeightKey);

    BOOL isDark    = darkNum ? [darkNum boolValue] : NO;
    int variantId  = variantNum ? [variantNum intValue] : 11;

    // 1. 同步亮/暗棱镜模式 (Aqua vs DarkAqua)
    NSAppearanceName appName = isDark ? NSAppearanceNameDarkAqua : NSAppearanceNameAqua;
    NSAppearance *targetApp = [NSAppearance appearanceNamed:appName];
    if (targetApp && glassView.appearance != targetApp) {
        glassView.appearance = targetApp;
    }

    // 2. 确保在桌面层级 (-1) 和失去焦点时始终保持原生液态玻璃高光
    SEL subduedSel = NSSelectorFromString(@"set_subduedState:");
    if ([glassView respondsToSelector:subduedSel]) {
        ((void (*)(id, SEL, NSInteger))objc_msgSend)(glassView, subduedSel, 0);
    }

    // 3. 基于 v11 水滴透镜基础核心，定制 5 款高品质液态玻璃衍生风格及选项微调
    CALayer *backdrop = gotify_find_backdrop_layer(glassView.layer);
    if (backdrop) {
        @try {
            double baseRefract = -50.0;
            double baseHeight  = 20.0;
            double baseOuter   = 10.0;
            double baseBlur    = 0.0;
            double faceOpacity = 0.0;
            double faceDarken  = 0.0;
            double highlightMul = 1.0;

            if (variantId == 11) {
                // 💧 晶透水滴 (原生纯粹): 零模糊、纯净折射、亮白水滴双亮边
                baseRefract = -50.0;
                baseHeight  = 20.0;
                baseOuter   = 10.0;
                baseBlur    = 0.0;
                faceOpacity = 0.0;
                highlightMul = 1.0;
            } else if (variantId == 13) {
                // 🔍 深弧透镜 (强力折射): -76 深度折射，28px 宽光弧，凸显水滴表面张力
                baseRefract = -76.0;
                baseHeight  = 28.0;
                baseOuter   = 16.0;
                baseBlur    = 0.0;
                faceOpacity = 0.0;
                highlightMul = 1.20;
            } else if (variantId == 14) {
                // 🌈 浮光色散 (棱镜彩虹): 水滴透镜叠加边缘微弱色散
                baseRefract = -58.0;
                baseHeight  = 22.0;
                baseOuter   = 12.0;
                baseBlur    = 0.0;
                faceOpacity = 0.05;
                highlightMul = 1.10;
                @try {
                    [backdrop setValue:@(-28.0) forKeyPath:@"filters.displacementMap.inputAmount"];
                } @catch (NSException *_) {}
            } else if (variantId == 15) {
                // 💎 黑曜水晶 (暗调微光): 深色水晶质感，高对比高光边框
                baseRefract = -65.0;
                baseHeight  = 24.0;
                baseOuter   = 12.0;
                baseBlur    = 1.5;
                faceDarken  = 0.18;
                faceOpacity = 0.35;
                highlightMul = 1.30;
            }

            // 自定义参数微调 (前端滑块实时输入)
            if (refractNum) {
                double rVal = [refractNum doubleValue]; // -20.0 ~ -90.0
                if (rVal != 0.0) baseRefract = rVal;
            }
            if (blurNum) {
                double bVal = [blurNum doubleValue]; // 0.0 ~ 12.0
                if (bVal >= 0.0) baseBlur = bVal;
            }
            if (highNum) {
                double hVal = [highNum doubleValue]; // 0.0 ~ 2.0 (倍率)
                if (hVal >= 0.0) highlightMul = hVal;
            }
            if (arcHeightNum) {
                double aVal = [arcHeightNum doubleValue]; // 8.0 ~ 36.0 (px)
                if (aVal > 0.0) baseHeight = aVal;
            }

            // 镜面高光反射与边缘双光弧参数调优：highlightMul 为 0 时彻底关停上下反光与浅色填充
            double baseKeyHighlight = 0.0;
            double keyHeight        = 0.0;
            double outerRefract     = 0.0;
            double lightenFill      = 0.0;

            if (highlightMul > 0.001) {
                baseKeyHighlight = 0.50 * highlightMul;
                keyHeight        = (baseHeight / 40.0) * sqrt(fmax(0.05, highlightMul));
                outerRefract     = baseOuter * highlightMul;
                lightenFill      = 0.675 * highlightMul;
            }

            [CATransaction begin];
            [CATransaction setDisableActions:YES];
            [backdrop setValue:@(baseRefract)        forKeyPath:@"filters.glassBackground.inputInnerRefractionAmount"];
            [backdrop setValue:@(baseHeight)         forKeyPath:@"filters.glassBackground.inputInnerRefractionHeight"];
            [backdrop setValue:@(outerRefract)       forKeyPath:@"filters.glassBackground.inputOuterRefractionAmount"];
            [backdrop setValue:@(10.0)               forKeyPath:@"filters.glassBackground.inputOuterRefractionHeight"];
            [backdrop setValue:@(baseBlur)           forKeyPath:@"filters.glassBackground.inputBlurRadius"];
            [backdrop setValue:@(faceOpacity)        forKeyPath:@"filters.glassBackground.inputFaceOpacity"];
            [backdrop setValue:@(faceDarken)         forKeyPath:@"filters.glassBackground.inputFaceDarkenAmount"];
            [backdrop setValue:@(lightenFill)        forKeyPath:@"filters.glassBackground.inputBlurFillLightenOpacity"];
            [backdrop setValue:@(baseKeyHighlight)   forKeyPath:@"filters.glassBackground.inputKeyFillHighlightAmount"];
            [backdrop setValue:@(keyHeight)          forKeyPath:@"filters.glassBackground.inputKeyFillHighlightHeight"];

            // 遍历整棵 CALayer 树，精准调节 Apple 原生 @2 CASDFLayer 高光棱镜层的透明度
            gotify_traverse_glass_layers(glassView.layer, highlightMul);
            gotify_kill_layer_animations(glassView.layer);
            [CATransaction commit];
            if (flush) {
                [CATransaction flush];
            }
        } @catch (NSException * _) {}
    }
    gotify_applying_tuning = NO;
}

static void gotify_apply_liquid_glass_tuning(NSView *glassView) {
    gotify_apply_liquid_glass_tuning_impl(glassView, YES);
}

static void (*orig_glass_windowChangedKeyState)(id, SEL) = NULL;
static void (*orig_glass_set_subduedState)(id, SEL, NSInteger) = NULL;
static void (*orig_glass_set_scrimState)(id, SEL, NSInteger) = NULL;
static void (*orig_glass_set_interactionState)(id, SEL, NSInteger) = NULL;

// Apple 在焦点/活跃状态切换时会异步重建玻璃图层并覆盖我们设置的高光参数，
// 同步重刷会被随后完成的内部更新冲掉。这里改为在状态切换后延迟多次重刷，
// 且仅在窗口非活跃（平态）时执行，保留激活瞬间 Apple 默认高光的观感。
// 注意：绝对不能在 AppKit 焦点切换调用栈内同步执行（其中的 CATransaction flush
// 会重入 tao 的重绘流程并与事件循环锁互相等待，直接死锁），必须全部异步。
static void gotify_schedule_tuning_restore(NSView *glassView) {
    if (!glassView) return;
    void (^applyIfIdle)(void) = ^{
        if (glassView.window) {
            gotify_apply_liquid_glass_tuning(glassView);
        }
    };
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(30 * NSEC_PER_MSEC)), dispatch_get_main_queue(), applyIfIdle);
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(120 * NSEC_PER_MSEC)), dispatch_get_main_queue(), applyIfIdle);
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(400 * NSEC_PER_MSEC)), dispatch_get_main_queue(), applyIfIdle);
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(900 * NSEC_PER_MSEC)), dispatch_get_main_queue(), applyIfIdle);
}

// 焦点丢失后的过渡动画通常持续 200~300ms，且由 Apple 在后续帧逐步施加。
// 这里在丢失焦点后以约 60fps 连续压住用户设定值，确保过渡期间不出现默认高光闪跳。
static void gotify_restore_guard_loop(NSView *glassView, int remaining) {
    if (!glassView || remaining <= 0) return;
    // 全部异步执行：绝不能在 AppKit 焦点切换调用栈内同步 flush（会死锁）。
    // 不做 key 状态判断：Apple 的状态翻转与通知时序存在竞态，条件判断会导致偶发漏刷。
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(16 * NSEC_PER_MSEC)), dispatch_get_main_queue(), ^{
        if (!glassView.window) return;
        gotify_apply_liquid_glass_tuning_impl(glassView, YES);
        gotify_restore_guard_loop(glassView, remaining - 1);
    });
}

// 焦点切走时先在当前调用栈内同步写入设定值（禁止 flush，不会死锁），
// 让同一渲染事务直接带上我们的高光参数，避免先闪一帧 Apple 默认高光；
// 随后再延迟多次兜底重刷，防止 Apple 内部异步重建把参数覆盖掉。
static void gotify_restore_tuning_now(NSView *glassView) {
    if (!glassView || !glassView.window) return;
    gotify_apply_liquid_glass_tuning_impl(glassView, NO);
}

static void gotify_glass_windowChangedKeyState(id self, SEL _cmd) {
    // 不再调用原始实现：Apple 在窗口焦点变化时会改写玻璃高光并附带过渡动画，
    // 这正是"点其他窗口时闪一下"的来源。外观已由 subduedState 恒 0 + 用户调优参数完全接管。
    // 调优过程内部触发的状态变化不再排重刷任务，避免延迟任务无限增殖
    if (gotify_applying_tuning) return;
    gotify_restore_tuning_now((NSView *)self);
    gotify_restore_guard_loop((NSView *)self, 40);
    gotify_schedule_tuning_restore((NSView *)self);
}

static void gotify_glass_set_subduedState(id self, SEL _cmd, NSInteger state) {
    // 强制传 0（常态/激活状态），彻底杜绝 Apple 内部在窗口失焦时灰暗化或在激活瞬间的滤镜重置闪烁
    if (orig_glass_set_subduedState) {
        orig_glass_set_subduedState(self, _cmd, 0);
    }
    if (gotify_applying_tuning) return;
    gotify_restore_tuning_now((NSView *)self);
    gotify_restore_guard_loop((NSView *)self, 40);
    gotify_schedule_tuning_restore((NSView *)self);
}

static void gotify_glass_set_scrimState(id self, SEL _cmd, NSInteger state) {
    // 遮罩层随窗口焦点切换而变化，会产生整面板亮度突跳；锁定为 0（无遮罩）
    if (orig_glass_set_scrimState) {
        orig_glass_set_scrimState(self, _cmd, 0);
    }
    if (gotify_applying_tuning) return;
    gotify_restore_tuning_now((NSView *)self);
}

static void gotify_glass_set_interactionState(id self, SEL _cmd, NSInteger state) {
    // 点击时的按压高光（即使反光透明度为 0 也会出现），锁定为 0（无交互态）
    if (orig_glass_set_interactionState) {
        orig_glass_set_interactionState(self, _cmd, 0);
    }
    if (gotify_applying_tuning) return;
    gotify_restore_tuning_now((NSView *)self);
}

static void gotify_swizzle_glass_effect_view(Class glassCls) {
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        SEL selKey = NSSelectorFromString(@"_windowChangedKeyState");
        Method mKey = class_getInstanceMethod(glassCls, selKey);
        if (mKey) {
            orig_glass_windowChangedKeyState = (void (*)(id, SEL))method_getImplementation(mKey);
            method_setImplementation(mKey, (IMP)gotify_glass_windowChangedKeyState);
        }
        SEL selSub = NSSelectorFromString(@"set_subduedState:");
        Method mSub = class_getInstanceMethod(glassCls, selSub);
        if (mSub) {
            orig_glass_set_subduedState = (void (*)(id, SEL, NSInteger))method_getImplementation(mSub);
            method_setImplementation(mSub, (IMP)gotify_glass_set_subduedState);
        }

        SEL selScrim = NSSelectorFromString(@"set_scrimState:");
        Method mScrim = class_getInstanceMethod(glassCls, selScrim);
        if (mScrim) {
            orig_glass_set_scrimState = (void (*)(id, SEL, NSInteger))method_getImplementation(mScrim);
            method_setImplementation(mScrim, (IMP)gotify_glass_set_scrimState);
        }

        SEL selInter = NSSelectorFromString(@"set_interactionState:");
        Method mInter = class_getInstanceMethod(glassCls, selInter);
        if (mInter) {
            orig_glass_set_interactionState = (void (*)(id, SEL, NSInteger))method_getImplementation(mInter);
            method_setImplementation(mInter, (IMP)gotify_glass_set_interactionState);
        }
    });
}

static NSView* gotify_create_or_rebuild_glass_view(NSWindow *win, double cornerRadius, double insetPt, int variantId, BOOL isDark, BOOL forceRebuild) {
    if (!win || !win.contentView) return nil;
    NSView *contentView = win.contentView;
    contentView.wantsLayer = YES;
    gotify_make_webviews_transparent(contentView);

    if (cornerRadius > 0.0) {
        objc_setAssociatedObject(contentView, kGotifyGlassCornerKey, @(cornerRadius), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    } else {
        NSNumber *savedCr = objc_getAssociatedObject(contentView, kGotifyGlassCornerKey);
        cornerRadius = savedCr ? [savedCr doubleValue] : 26.0;
    }

    if (insetPt >= 0.0) {
        objc_setAssociatedObject(contentView, kGotifyGlassInsetKey, @(insetPt), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    } else {
        NSNumber *savedInset = objc_getAssociatedObject(contentView, kGotifyGlassInsetKey);
        insetPt = savedInset ? [savedInset doubleValue] : 0.0;
    }

    Class glassCls = NSClassFromString(@"NSGlassEffectView");
    if (glassCls) {
        NSView *existing = nil;
        for (NSView *sub in [contentView.subviews copy]) {
            if ([sub isKindOfClass:glassCls]) {
                existing = sub;
                break;
            }
        }

        gotify_swizzle_glass_effect_view(glassCls);

        static const void *kGotifyGlassObsKey = &kGotifyGlassObsKey;
        if (!objc_getAssociatedObject(win, kGotifyGlassObsKey)) {
            objc_setAssociatedObject(win, kGotifyGlassObsKey, @YES, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            NSNotificationCenter *nc = [NSNotificationCenter defaultCenter];
            void (^reapplyBlock)(NSNotification *) = ^(NSNotification * _Nonnull note) {
                // 焦点切换后 Apple 会异步重建图层，使用延迟多次重刷把用户设定值写回去
                if (!win || !win.contentView) return;
                Class gCls = NSClassFromString(@"NSGlassEffectView");
                for (NSView *s in win.contentView.subviews) {
                    if ([s isKindOfClass:gCls]) {
                        gotify_restore_tuning_now(s);
                        gotify_restore_guard_loop(s, 40);
                        gotify_schedule_tuning_restore(s);
                        break;
                    }
                }
            };
            [nc addObserverForName:NSWindowDidBecomeKeyNotification object:win queue:[NSOperationQueue mainQueue] usingBlock:reapplyBlock];
            [nc addObserverForName:NSWindowDidResignKeyNotification object:win queue:[NSOperationQueue mainQueue] usingBlock:reapplyBlock];
            [nc addObserverForName:NSWindowDidBecomeMainNotification object:win queue:[NSOperationQueue mainQueue] usingBlock:reapplyBlock];
            [nc addObserverForName:NSWindowDidResignMainNotification object:win queue:[NSOperationQueue mainQueue] usingBlock:reapplyBlock];
        }

        if (existing && !forceRebuild) {
            objc_setAssociatedObject(existing, kGotifyGlassVariantKey, @(variantId), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            objc_setAssociatedObject(existing, kGotifyGlassDarkKey,    @(isDark),    OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            objc_setAssociatedObject(win,      kGotifyGlassVariantKey, @(variantId), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            objc_setAssociatedObject(win,      kGotifyGlassDarkKey,    @(isDark),    OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            if (insetPt >= 0.0) {
                objc_setAssociatedObject(existing, kGotifyGlassInsetKey, @(insetPt), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            }
            NSAppearanceName appName = isDark ? NSAppearanceNameDarkAqua : NSAppearanceNameAqua;
            NSAppearance *targetApp = [NSAppearance appearanceNamed:appName];
            if (targetApp && existing.appearance != targetApp) {
                existing.appearance = targetApp;
            }
            gotify_apply_liquid_glass_tuning(existing);
            return existing;
        }

        if (existing) {
            [existing removeFromSuperview];
        }

        NSRect initFrame = NSInsetRect(contentView.bounds, (CGFloat)insetPt, (CGFloat)insetPt);
        if (initFrame.size.width < 10 || initFrame.size.height < 10) {
            initFrame = contentView.bounds;
        }

        NSView *glassView = [[glassCls alloc] initWithFrame:initFrame];
        glassView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;

        NSAppearanceName appName = isDark ? NSAppearanceNameDarkAqua : NSAppearanceNameAqua;
        glassView.appearance = [NSAppearance appearanceNamed:appName];

        SEL crSel = NSSelectorFromString(@"setCornerRadius:");
        if ([glassView respondsToSelector:crSel]) {
            ((void (*)(id, SEL, CGFloat))objc_msgSend)(glassView, crSel, (CGFloat)cornerRadius);
        }

        SEL varSel = NSSelectorFromString(@"set_variant:");
        if ([glassView respondsToSelector:varSel]) {
            // 所有衍生风格均基于真正的 v11 晶莹水滴透镜核心底座构建，拥有 Apple 原生 @2 和 @3 上下高光棱镜层
            ((void (*)(id, SEL, NSInteger))objc_msgSend)(glassView, varSel, 11);
        }

        SEL interSel = NSSelectorFromString(@"setEffectIsInteractive:");
        if ([glassView respondsToSelector:interSel]) {
            ((void (*)(id, SEL, BOOL))objc_msgSend)(glassView, interSel, NO);
        }
        SEL interSel2 = NSSelectorFromString(@"set_effectIsInteractive:");
        if ([glassView respondsToSelector:interSel2]) {
            ((void (*)(id, SEL, BOOL))objc_msgSend)(glassView, interSel2, NO);
        }

        objc_setAssociatedObject(glassView, kGotifyGlassVariantKey, @(variantId), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
        objc_setAssociatedObject(glassView, kGotifyGlassDarkKey,    @(isDark),    OBJC_ASSOCIATION_RETAIN_NONATOMIC);
        objc_setAssociatedObject(glassView, kGotifyGlassInsetKey,   @(insetPt),   OBJC_ASSOCIATION_RETAIN_NONATOMIC);
        objc_setAssociatedObject(win,       kGotifyGlassVariantKey, @(variantId), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
        objc_setAssociatedObject(win,       kGotifyGlassDarkKey,    @(isDark),    OBJC_ASSOCIATION_RETAIN_NONATOMIC);

        [contentView addSubview:glassView positioned:NSWindowBelow relativeTo:nil];
        [glassView layoutSubtreeIfNeeded];
        [CATransaction flush];
        gotify_apply_liquid_glass_tuning(glassView);
        [CATransaction flush];
        return glassView;
    }

    // Fallback for macOS < 26 where NSGlassEffectView does not exist: use NSVisualEffectView
    for (NSView *sub in contentView.subviews) {
        if ([sub isKindOfClass:[NSVisualEffectView class]]) {
            return sub;
        }
    }
    NSRect initFrame = NSInsetRect(contentView.bounds, (CGFloat)insetPt, (CGFloat)insetPt);
    NSVisualEffectView *vev = [[NSVisualEffectView alloc] initWithFrame:initFrame];
    vev.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    vev.blendingMode = NSVisualEffectBlendingModeBehindWindow;
    vev.material = NSVisualEffectMaterialHUDWindow;
    vev.state = NSVisualEffectStateActive;
    vev.wantsLayer = YES;
    vev.layer.cornerRadius = (CGFloat)cornerRadius;
    vev.layer.masksToBounds = YES;
    [contentView addSubview:vev positioned:NSWindowBelow relativeTo:nil];
    return vev;
}

void macos_install_liquid_glass(void* ns_window_ptr, double corner_radius, double inset_pt) {
    if (!ns_window_ptr) return;
    void (^block)(void) = ^{
        @autoreleasepool {
            NSWindow *win = (__bridge NSWindow *)ns_window_ptr;
            // Default to v11 (Pure Water-Droplet Lens)
            gotify_create_or_rebuild_glass_view(win, corner_radius, inset_pt, 11, NO, YES);
        }
    };
    if ([NSThread isMainThread]) {
        block();
    } else {
        dispatch_async(dispatch_get_main_queue(), block);
    }
}

void macos_update_liquid_glass(void* ns_window_ptr, int active, double opacity, int is_dark, int variant_id) {
    (void)active;
    (void)opacity;
    if (!ns_window_ptr) return;
    void (^block)(void) = ^{
        @autoreleasepool {
            NSWindow *win = (__bridge NSWindow *)ns_window_ptr;
            int targetVariant = (variant_id >= 0) ? variant_id : 11;
            // 切换预设风格时，重置前端手动微调覆盖，以便精确展现预设光学质感
            Class glassCls = NSClassFromString(@"NSGlassEffectView");
            if (win && win.contentView) {
                for (NSView *sub in win.contentView.subviews) {
                    if ([sub isKindOfClass:glassCls]) {
                        NSNumber *currVariant = objc_getAssociatedObject(sub, kGotifyGlassVariantKey);
                        if (!currVariant) {
                            currVariant = objc_getAssociatedObject(win, kGotifyGlassVariantKey);
                        }
                        if (currVariant && [currVariant intValue] != targetVariant) {
                            // 仅当真正切换为不同风格预设时，才重置该 View 上的手动微调参数
                            objc_setAssociatedObject(sub, kGotifyGlassRefractKey,   nil, OBJC_ASSOCIATION_ASSIGN);
                            objc_setAssociatedObject(sub, kGotifyGlassBlurKey,      nil, OBJC_ASSOCIATION_ASSIGN);
                            objc_setAssociatedObject(sub, kGotifyGlassHighKey,      nil, OBJC_ASSOCIATION_ASSIGN);
                            objc_setAssociatedObject(sub, kGotifyGlassArcHeightKey, nil, OBJC_ASSOCIATION_ASSIGN);
                            objc_setAssociatedObject(win, kGotifyGlassRefractKey,   nil, OBJC_ASSOCIATION_ASSIGN);
                            objc_setAssociatedObject(win, kGotifyGlassBlurKey,      nil, OBJC_ASSOCIATION_ASSIGN);
                            objc_setAssociatedObject(win, kGotifyGlassHighKey,      nil, OBJC_ASSOCIATION_ASSIGN);
                            objc_setAssociatedObject(win, kGotifyGlassArcHeightKey, nil, OBJC_ASSOCIATION_ASSIGN);
                        }
                        break;
                    }
                }
            }
            gotify_create_or_rebuild_glass_view(win, -1.0, -1.0, targetVariant, is_dark != 0, NO);
        }
    };
    if ([NSThread isMainThread]) {
        block();
    } else {
        dispatch_async(dispatch_get_main_queue(), block);
    }
}

void macos_tune_liquid_glass(void* ns_window_ptr, double refraction, double blur, double highlight, double arc_height) {
    if (!ns_window_ptr) return;
    void (^block)(void) = ^{
        @autoreleasepool {
            NSWindow *win = (__bridge NSWindow *)ns_window_ptr;
            if (!win || !win.contentView) return;
            if (refraction != 0.0) {
                objc_setAssociatedObject(win, kGotifyGlassRefractKey, @(refraction), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            }
            if (blur >= 0.0) {
                objc_setAssociatedObject(win, kGotifyGlassBlurKey, @(blur), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            }
            if (highlight >= 0.0) {
                objc_setAssociatedObject(win, kGotifyGlassHighKey, @(highlight), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            }
            if (arc_height > 0.0) {
                objc_setAssociatedObject(win, kGotifyGlassArcHeightKey, @(arc_height), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            }
            Class glassCls = NSClassFromString(@"NSGlassEffectView");
            for (NSView *sub in win.contentView.subviews) {
                if ([sub isKindOfClass:glassCls]) {
                    if (refraction != 0.0) {
                        objc_setAssociatedObject(sub, kGotifyGlassRefractKey, @(refraction), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
                    }
                    if (blur >= 0.0) {
                        objc_setAssociatedObject(sub, kGotifyGlassBlurKey, @(blur), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
                    }
                    if (highlight >= 0.0) {
                        objc_setAssociatedObject(sub, kGotifyGlassHighKey, @(highlight), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
                    }
                    if (arc_height > 0.0) {
                        objc_setAssociatedObject(sub, kGotifyGlassArcHeightKey, @(arc_height), OBJC_ASSOCIATION_RETAIN_NONATOMIC);
                    }
                    gotify_apply_liquid_glass_tuning(sub);
                    break;
                }
            }
        }
    };
    if ([NSThread isMainThread]) {
        block();
    } else {
        dispatch_async(dispatch_get_main_queue(), block);
    }
}


