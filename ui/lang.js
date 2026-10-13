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
        'pinWindow': '窗口置顶 (悬浮在其他界面之上)',
        'unpinWindow': '取消置顶 (当前置顶)',
        'windowPinned': '已开启窗口置顶',
        'windowUnpinned': '已关闭窗口置顶',
        'desktopWidget': '桌面小组件',
        'enterDesktopWidget': '钉在桌面 (小组件模式)',
        'exitDesktopWidget': '还原完整窗口',
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
        'showInDock': '在 Dock 中显示',
        'showInDockDesc': '在 Dock 栏显示应用图标',
        'showInDockEnabled': '已在 Dock 中显示',
        'showInDockDisabled': '已从 Dock 中隐藏',
        'showInDockFailed': '设置 Dock 显示失败',
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
        'pinWindow': 'Pin Window (Always on Top)',
        'unpinWindow': 'Unpin Window (Normal Level)',
        'windowPinned': 'Window pinned on top',
        'windowUnpinned': 'Window unpinned',
        'desktopWidget': 'Desktop Widget',
        'enterDesktopWidget': 'Pin to Desktop (Widget Mode)',
        'exitDesktopWidget': 'Restore Full Window',
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
        'showInDock': 'Show in Dock',
        'showInDockDesc': 'Display app icon in the Dock',
        'showInDockEnabled': 'Now showing in Dock',
        'showInDockDisabled': 'Now hidden from Dock',
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
(function() {
    const isWindows = typeof navigator !== 'undefined' && (
        /windows|win32/i.test(navigator.userAgent) || (navigator.platform && navigator.platform.indexOf('Win') !== -1)
    );
    if (isWindows) {
        if (translations['zh-CN']) {
            translations['zh-CN']['autoLaunchDesc'] = '登录 Windows 时自动启动 Gotify';
            translations['zh-CN']['showInDock'] = '在任务栏中显示';
            translations['zh-CN']['showInDockDesc'] = '在系统任务栏显示应用图标（关闭则仅在托盘常驻）';
            translations['zh-CN']['showInDockEnabled'] = '已在任务栏中显示';
            translations['zh-CN']['showInDockDisabled'] = '已从任务栏中隐藏（仅在托盘常驻）';
            translations['zh-CN']['showInDockFailed'] = '设置任务栏显示失败';
        }
        if (translations['en']) {
            translations['en']['autoLaunchDesc'] = 'Auto-start Gotify when you log in to Windows';
            translations['en']['showInDock'] = 'Show in Taskbar';
            translations['en']['showInDockDesc'] = 'Display app icon in the Taskbar (stay in tray if off)';
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
