// i18n 翻译系统
const translations = {
    'zh-CN': {
        // 通用
        'settings': '设置',
        'openGotify': '网页打开 Gotify',
        'toggleTheme': '切换主题',
        'toggleThemeDbl': '双击跟随系统',
        'followingSystem': '跟随系统主题',
        'customTheme': '自定义主题',

        // 通知
        'emptyState': '暂无通知',
        'readMore': 'READ MORE',
        'readLess': 'READ LESS',
        'clearAll': '清除所有（保留置顶）',
        'pin': '置顶',
        'unpin': '取消置顶',
        'pinWindow': '悬浮置顶 (前台)',
        'unpinWindow': '常驻桌面 (底层)',
        'windowPinned': '已开启悬浮置顶',
        'windowUnpinned': '已切换常驻桌面',
        'adjustOpacity': '调节玻璃透明度',
        'delete': '删除',
        'new': 'NEW',
        'priorityHigh': '高',
        'priorityMedium': '中',
        'priorityLow': '低',

        // 设置页面
        'serverSettings': 'Gotify 服务器设置',
        'serverSettingsDesc': '配置 Gotify 服务器连接信息',
        'connectionStatus': '连接状态',
        'connected': '已连接',
        'disconnected': '未连接',
        'serverAddress': '服务器地址',
        'serverAddressPlaceholder': 'http://your-gotify-server:8080',
        'clientToken': '客户端 Token (Clients token)',
        'clientTokenPlaceholder': '输入你的客户端 token',
        'testConnection': '测试连接',
        'saveSettings': '保存设置',
        'enterValidUrl': '请输入有效的服务器地址',

        // 应用设置
        'appSettings': '应用设置',
        'showInDock': '在 Dock 栏中显示图标',
        'showInDockDesc': '默认不出现在 Dock 栏（仅常驻菜单栏后台）；打开后才在 Dock 栏显示',
        'showInDockEnabled': '已在 Dock 栏中显示图标',
        'showInDockDisabled': '已从 Dock 栏中隐藏（仅常驻菜单栏后台）',
        'showInDockFailed': '设置 Dock 栏显示失败',
        'autoLaunch': '开机自启动',
        'autoLaunchDesc': '登录 Mac 时自动启动 Gotify',

        // 声音设置
        'soundSettings': '声音设置',
        'enableSound': '开启通知声音',
        'enableSoundDesc': '收到 Gotify 通知时播放提示音',
        'soundThreshold': '声音触发门槛',
        'soundThresholdDesc': '仅当通知优先级达到该值时播放声音',
        'allPriorities': '所有通知 (优先级 >= 0)',
        'mediumHighPriorities': '中高优先级 (优先级 >= 5)',
        'highOnly': '仅高优先级 (优先级 >= 8)',
        'testSound': '测试声音',

        // 新消息未读特效
        'unreadEffectSettings': '新消息特效设置',
        'unreadEffect': '未读视觉特效',
        'unreadEffectDesc': '新通知持续展示该特效，鼠标悬停（移动过去）后特效消失并标记已阅',
        'unreadBounceTitle': '未读消息律动弹跳',
        'unreadBounceDesc': '未读通知以物理弹性规律起伏弹跳，移入鼠标已阅后停止',
        'unreadBounceAmp': '弹跳幅度调节',
        'bounceAmpGentle': '微弱 (3px)',
        'bounceAmpMedium': '适中 (6px)',
        'bounceAmpProminent': '明显 (10px)',
        'bounceAmpStrong': '强烈 (14px)',
        'previewEffect': '即时预览',
        'effect_aurora': '极光霓虹流光 (推荐)',
        'effect_pulse': '呼吸脉动光晕',
        'effect_beacon': '科技信标高亮',
        'effect_ripple': '水波微澜涟漪',
        'effect_border_stream': '跑马灯炫彩边框',
        'effect_none': '关闭特效 (无动画)',
        'unreadEffectSampleTitle': '新通知特效演示',
        'unreadEffectSampleContent': '鼠标移动到该卡片上方，特效将优雅淡出消失表示已阅。',

        // 连接状态
        'connecting': '正在连接...',
        'reconnecting': '断开重连中...',
        'reconnect': '立即重连',

        // 状态消息
        'fillServerInfo': '请填写完整的服务器信息',
        'testingConnection': '正在测试连接...',
        'connectionSuccess': '连接成功',
        'connectionFailed': '连接失败',
        'settingsSaved': '设置已保存！',
        'settingsSaveFailed': '保存设置失败',
        'configLoadFailed': '获取配置失败',
        'autoLaunchEnabled': '已开启开机自启动',
        'autoLaunchDisabled': '已关闭开机自启动',
        'autoLaunchFailed': '设置开机启动失败',
        'autoLaunchGetFailed': '获取开机启动状态失败',
        'tauriUnavailable': 'Tauri API 不可用',
    },
    'en': {
        // Common
        'settings': 'Settings',
        'openGotify': 'Open Gotify in Browser',
        'toggleTheme': 'Toggle Theme',
        'toggleThemeDbl': 'Double-click to follow system',
        'followingSystem': 'Following system theme',
        'customTheme': 'Custom theme',

        // Notifications
        'emptyState': 'No notifications',
        'readMore': 'READ MORE',
        'readLess': 'READ LESS',
        'clearAll': 'Clear All (keep pinned)',
        'pin': 'Pin',
        'unpin': 'Unpin',
        'pinWindow': 'Always on Top (Foreground)',
        'unpinWindow': 'Pin to Desktop (Background)',
        'windowPinned': 'Window set to always on top',
        'windowUnpinned': 'Window set to desktop level',
        'adjustOpacity': 'Adjust Glass Opacity',
        'delete': 'Delete',
        'new': 'NEW',
        'priorityHigh': 'High',
        'priorityMedium': 'Medium',
        'priorityLow': 'Low',

        // Settings page
        'serverSettings': 'Gotify Server Settings',
        'serverSettingsDesc': 'Configure Gotify server connection',
        'connectionStatus': 'Connection Status',
        'connected': 'Connected',
        'disconnected': 'Disconnected',
        'serverAddress': 'Server Address',
        'serverAddressPlaceholder': 'http://your-gotify-server:8080',
        'clientToken': 'Client Token',
        'clientTokenPlaceholder': 'Enter your client token',
        'testConnection': 'Test Connection',
        'saveSettings': 'Save Settings',
        'autoLaunch': 'Launch on Startup',
        'autoLaunchDesc': 'Auto-start Gotify when you log in',

        // Application settings
        'appSettings': 'Application Settings',
        'showInDock': 'Show Icon in Dock',
        'showInDockDesc': 'Hidden by default (stays in menu bar like background services); turn on to show in Dock',
        'showInDockEnabled': 'Now showing in Dock',
        'showInDockDisabled': 'Now hidden from Dock (menu bar only)',
        'showInDockFailed': 'Failed to toggle Dock visibility',

        // Sound settings
        'soundSettings': 'Sound Settings',
        'enableSound': 'Notification Sound',
        'enableSoundDesc': 'Play sound when receiving notifications',
        'soundThreshold': 'Sound Priority Threshold',
        'soundThresholdDesc': 'Only play sound for notifications with this priority',
        'allPriorities': 'All Notifications (>= 0)',
        'mediumHighPriorities': 'Medium & High (>= 5)',
        'highOnly': 'High Only (>= 8)',
        'testSound': 'Test Sound',

        // Unread Effect Settings
        'unreadEffectSettings': 'Unread Effect Settings',
        'unreadEffect': 'New Message Animation',
        'unreadEffectDesc': 'New notifications keep this effect until you hover over them to mark as read',
        'unreadBounceTitle': 'Unread Notification Bounce',
        'unreadBounceDesc': 'Bounces unread notification cards rhythmically until hovered to mark as read',
        'unreadBounceAmp': 'Bounce Amplitude',
        'bounceAmpGentle': 'Subtle (3px)',
        'bounceAmpMedium': 'Medium (6px)',
        'bounceAmpProminent': 'Prominent (10px)',
        'bounceAmpStrong': 'Strong (14px)',
        'previewEffect': 'Live Preview',
        'effect_aurora': 'Aurora Neon Flow (Recommended)',
        'effect_pulse': 'Breathing Pulse Glow',
        'effect_beacon': 'Tech Beacon Spotlight',
        'effect_ripple': 'Gentle Ripple Wave',
        'effect_border_stream': 'Rainbow Marquee Border',
        'effect_none': 'Disabled (No Animation)',
        'unreadEffectSampleTitle': 'Unread Effect Preview',
        'unreadEffectSampleContent': 'Hover cursor over this card to watch the effect smoothly fade away as marked read.',

        // Connection status
        'connecting': 'Connecting...',
        'reconnecting': 'Reconnecting...',
        'reconnect': 'Reconnect Now',

        // Status messages
        'fillServerInfo': 'Please fill in all server information',
        'enterValidUrl': 'Please enter a valid server address',
        'testingConnection': 'Testing connection...',
        'connectionSuccess': 'Connection successful',
        'connectionFailed': 'Connection failed',
        'settingsSaved': 'Settings saved!',
        'settingsSaveFailed': 'Failed to save settings',
        'configLoadFailed': 'Failed to load configuration',
        'autoLaunchEnabled': 'Auto-launch enabled',
        'autoLaunchDisabled': 'Auto-launch disabled',
        'autoLaunchFailed': 'Failed to set auto-launch',
        'autoLaunchGetFailed': 'Failed to get auto-launch status',
        'tauriUnavailable': 'Tauri API unavailable',
    }
};

// 平台自适应文案调整
(function () {
    const isWindows = typeof navigator !== 'undefined' && (
        /windows|win32/i.test(navigator.userAgent) || (navigator.platform && navigator.platform.indexOf('Win') !== -1)
    );
    if (isWindows) {
        if (translations['zh-CN']) {
            translations['zh-CN']['autoLaunchDesc'] = '登录 Windows 时自动启动 Gotify';
            translations['zh-CN']['showInDock'] = '在任务栏中显示图标';
            translations['zh-CN']['showInDockDesc'] = '默认不出现在任务栏（仅常驻右下角托盘后台）；打开后才在任务栏显示';
            translations['zh-CN']['showInDockEnabled'] = '已在任务栏中显示图标';
            translations['zh-CN']['showInDockDisabled'] = '已从任务栏中隐藏（仅常驻托盘后台）';
            translations['zh-CN']['showInDockFailed'] = '设置任务栏显示失败';
        }
        if (translations['en']) {
            translations['en']['autoLaunchDesc'] = 'Auto-start Gotify when you log in to Windows';
            translations['en']['showInDock'] = 'Show Icon in Taskbar';
            translations['en']['showInDockDesc'] = 'Hidden by default (stays in system tray like background services); turn on to show in Taskbar';
            translations['en']['showInDockEnabled'] = 'Now showing in Taskbar';
            translations['en']['showInDockDisabled'] = 'Now hidden from Taskbar (tray only)';
            translations['en']['showInDockFailed'] = 'Failed to toggle Taskbar visibility';
        }
    }
})();

// 检测系统语言
function detectLanguage() {
    const saved = localStorage.getItem('lang');
    if (saved && translations[saved]) return saved;

    const nav = navigator.language || navigator.userLanguage || '';
    if (nav.startsWith('zh')) return 'zh-CN';
    return 'en';
}

// 当前语言
let currentLang = detectLanguage();

// 翻译函数
function t(key) {
    return translations[currentLang]?.[key] || translations['en']?.[key] || key;
}

// 切换语言
function setLang(lang) {
    if (translations[lang]) {
        currentLang = lang;
        localStorage.setItem('lang', lang);
    }
}
