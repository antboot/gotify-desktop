(function () {
    function report(kind, detail) {
        try {
            if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke) {
                window.__TAURI_INTERNALS__.invoke('report_js_error', { message: '[' + kind + '] ' + detail });
            }
        } catch (_) {}
    }
    window.addEventListener('error', function (e) {
        report('error', (e.message || 'unknown') + ' @ ' + (e.filename || '') + ':' + (e.lineno || 0));
    });
    window.addEventListener('unhandledrejection', function (e) {
        var r = e.reason;
        report('rejection', (r && r.message) ? r.message : String(r));
    });
})();
