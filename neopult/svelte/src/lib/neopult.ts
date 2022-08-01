import { writable } from 'svelte/store';

export enum SocketError {
    STORED_PASSWORD_INCORRECT,
    PASSWORD_INCORRECT,
    AUTH_TIMEOUT,
}

export interface SocketConnectionState {
    connecting: boolean;
    tryingReconnect: boolean;
    reconnectTry: number;
    reconnectInMs: number;
    connected: boolean;
    initialConnect: boolean;
    error: SocketError | null;
}

export interface Action {
    name: string;
    displayName: string;
    active: boolean;
}

export interface Module {
    name: string;
    displayName: string;
    status: string;
    message: string;
    actions: {
        [name: string]: Action;
    };
}

export interface PluginInstance {
    name: string;
    modules: {
        [name: string]: Module;
    };
}

export interface NeopultState {
    pluginInstances: {
        [name: string]: PluginInstance;
    };
}

export const socketConnectionStore = writable<SocketConnectionState>({
    connecting: false,
    tryingReconnect: false,
    reconnectTry: 0,
    reconnectInMs: 0,
    connected: false,
    initialConnect: true,
    error: null,
});

export const neopultStore = writable<NeopultState>({
    pluginInstances: {},
});

// NOTE: Make sure to adjust the timeout when changing the ping interval on the server
const CONNECTION_TIMEOUT_MS = 10000;
const RECONNECT_INTERVALS_MS = [1000, 3000, 10000];

const SOCKET_DISCONNECT_REASON_AUTH = 'auth';
const SOCKET_DISCONNECT_REASON_AUTH_TIMEOUT = 'auth_timeout';
const SOCKET_DISCONNECT_REASON_CLIENT_LOGOUT = 'client_logout';

let socket: WebSocket;
let requestId = 0;
let cachedPassword = '';

let heartbeatTimeout: NodeJS.Timeout;
let reconnectTimeout: NodeJS.Timeout;
let reconnectUpdateInterval: NodeJS.Timer;

const heartbeat = () => {
    clearTimeout(heartbeatTimeout);
    heartbeatTimeout = setTimeout(() => {
        console.log('connection timed out');
        socket.close();
        handleDisconnect('');
    }, CONNECTION_TIMEOUT_MS);
};

export const reconnect = () => {
    clearReconnectTimers();
    connect(cachedPassword);
};

const scheduleReconnect = (reconnectTry: number) => {
    let reconnectIntervalIdx = reconnectTry - 1;
    if (reconnectIntervalIdx >= RECONNECT_INTERVALS_MS.length) {
        reconnectIntervalIdx = RECONNECT_INTERVALS_MS.length - 1;
    }
    let reconnectIn = RECONNECT_INTERVALS_MS[reconnectIntervalIdx];
    reconnectTimeout = setTimeout(reconnect, reconnectIn);
    reconnectUpdateInterval = setInterval(() => {
        reconnectIn -= 1000;
        socketConnectionStore.update((state) => {
            state.reconnectInMs = reconnectIn;
            return state;
        });
    }, 1000);
    return reconnectIn;
};

const clearReconnectTimers = () => {
    clearTimeout(reconnectTimeout);
    clearInterval(reconnectUpdateInterval);
}

export const logout = () => {
    clearReconnectTimers();
    socket.close();
    handleDisconnect(SOCKET_DISCONNECT_REASON_CLIENT_LOGOUT);
};

const handleDisconnect = (reason: string) => {
    socket.onopen = null;
    socket.onmessage = null;
    socket.onerror = null;
    socket.onclose = null;

    clearTimeout(heartbeatTimeout);

    socketConnectionStore.update((state) => {
        state.connecting = false;
        state.connected = false;

        if (reason === SOCKET_DISCONNECT_REASON_AUTH) {
            // TODO: If has stored password: Remove stored password and set reason accordingly
            state.error = SocketError.PASSWORD_INCORRECT;
            state.initialConnect = true;
        } else if (reason === SOCKET_DISCONNECT_REASON_AUTH_TIMEOUT) {
            state.error = SocketError.AUTH_TIMEOUT;
        } else if (reason === SOCKET_DISCONNECT_REASON_CLIENT_LOGOUT) {
            state.initialConnect = true;
            state.error = null;
        }

        const shouldReconnect =
            reason !== SOCKET_DISCONNECT_REASON_AUTH &&
            reason !== SOCKET_DISCONNECT_REASON_AUTH_TIMEOUT &&
            reason !== SOCKET_DISCONNECT_REASON_CLIENT_LOGOUT;
        if (shouldReconnect) {
            if (state.tryingReconnect) {
                state.reconnectTry += 1;
            } else {
                state.tryingReconnect = true;
                state.reconnectTry = 1;
            }
            state.reconnectInMs = scheduleReconnect(state.reconnectTry);
        } else {
            state.tryingReconnect = false;
        }

        return state;
    });
};

export const connect = (password: string) => {
    console.log('Connecting');
    socket = new WebSocket('ws://localhost:4205/ws');
    cachedPassword = password;

    socketConnectionStore.update((state) => {
        state.connecting = true;
        state.error = null;
        return state;
    });

    socket.onopen = (event) => {
        console.log('socket open', event);
        clearReconnectTimers();
        socket.send('Password ' + password);
    };

    socket.onmessage = (event) => {
        console.log('socket message', event.data);

        let msg;
        try {
            msg = JSON.parse(event.data);
        } catch (e) {
            console.error('parse error:', e);
            return;
        }

        if (msg == 'ping') {
            heartbeat();
            socket.send('"pong"');
        } else if (msg == 'pong') {
            heartbeat();
        } else if (msg.system_info) {
            socketConnectionStore.set({
                connecting: false,
                tryingReconnect: false,
                reconnectTry: 0,
                reconnectInMs: 0,
                connected: true,
                initialConnect: false,
                error: null,
            });
            cachedPassword = password;

            const neopultState: NeopultState = { pluginInstances: {} };
            for (const pluginInstance of msg.system_info.plugin_instances) {
                neopultState.pluginInstances[pluginInstance.name] = {
                    name: pluginInstance.name,
                    modules: {},
                };
                for (const module of pluginInstance.modules) {
                    neopultState.pluginInstances[pluginInstance.name].modules[module.name] = {
                        name: module.name,
                        displayName: module.display_name,
                        status: module.status,
                        message: module.message,
                        actions: {},
                    };
                    for (const action of module.actions) {
                        neopultState.pluginInstances[pluginInstance.name].modules[
                            module.name
                        ].actions[action.name] = {
                            name: action.name,
                            displayName: action.display_name || action.name,
                            active: false,
                        };
                    }
                    for (const actionName of module.active_actions) {
                        neopultState.pluginInstances[pluginInstance.name].modules[
                            module.name
                        ].actions[actionName].active = true;
                    }
                }
            }
            neopultStore.set(neopultState);
        } else if (msg.notification) {
            const notification = msg.notification;
            if (notification.module_status_update) {
                const update = notification.module_status_update;
                neopultStore.update((state) => {
                    const module =
                        state.pluginInstances[update.plugin_instance].modules[update.module];
                    module.status = update.new_status;
                    return state;
                });
            } else if (notification.module_message_update) {
                const update = notification.module_message_update;
                neopultStore.update((state) => {
                    const module =
                        state.pluginInstances[update.plugin_instance].modules[update.module];
                    module.message = update.new_message;
                    return state;
                });
            } else if (notification.module_active_actions_update) {
                const update = notification.module_active_actions_update;
                neopultStore.update((state) => {
                    const module =
                        state.pluginInstances[update.plugin_instance].modules[update.module];
                    for (const action of Object.values(module.actions)) {
                        action.active = false;
                    }
                    for (const actionName of update.new_active_actions) {
                        module.actions[actionName].active = true;
                    }
                    return state;
                });
            }
        }
    };

    socket.onerror = (event) => {
        console.error('socket error', event);
    };

    socket.onclose = (event) => {
        console.log('socket close', event);
        handleDisconnect(event.reason);
    };
};

export const callAction = (pluginInstance: string, module: string, action: string) => {
    const request = {
        request: {
            request_id: requestId.toString(),
            body: {
                call_action: {
                    plugin_instance: pluginInstance,
                    module,
                    action,
                },
            },
        },
    };
    requestId++;
    const json = JSON.stringify(request);
    socket.send(json);
};
