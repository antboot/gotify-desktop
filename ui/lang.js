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
