export async function listen(eventName, cb, options) {
    const internals = window.__TAURI_INTERNALS__;
    if (!internals || typeof internals.invoke !== 'function' || typeof internals.transformCallback !== 'function') {
        throw new Error('Tauri internals unavailable');
    }
    const target = (options && options.target) ? options.target : { kind: 'Any' };
    const handler = internals.transformCallback(cb);
    return internals.invoke('plugin:event|listen', { event: eventName, target, handler });
}

export default { listen };
