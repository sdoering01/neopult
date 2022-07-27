import { writable } from 'svelte/store';

export interface SocketConnectionState {
    connecting: boolean;
    connected: boolean;
    initialConnect: boolean;
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

let socket: WebSocket;
let requestId = 0;

export const socketConnectionStore = writable<SocketConnectionState>({
    connecting: false,
    connected: false,
    initialConnect: true,
});

export const neopultStore = writable<NeopultState>({
    pluginInstances: {},
});

const connect = () => {
    socket = new WebSocket('ws://localhost:4205/ws');

    socket.onopen = () => {
        console.log('socket open');

        // TODO: Read from user
        socket.send('Password neopult');
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
            // TODO: Heartbeat
            socket.send('"pong"');
        } else if (msg == 'pong') {
            // TODO: Heartbeat
        } else if (msg.system_info) {
            socketConnectionStore.set({
                connecting: false,
                connected: true,
                initialConnect: false,
            });

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

    socket.onclose = () => {
        console.log('socket close');

        socketConnectionStore.update((state) => ({
            connecting: false,
            connected: false,
            initialConnect: state.initialConnect,
        }));
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

connect();
