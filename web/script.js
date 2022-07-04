// NOTE: Make sure to adjust the timeout when changing the ping interval on the server
const CONNECTION_TIMEOUT_MS = 10000;
const RECONNECT_INTERVALS_MS = [1000, 3000, 10000];

let initialConnect = true;
let reconnecting = false;
let heartbeatTimeout;
let reconnectTry;
let reconnectTimeout;

const socketProtocol = window.location.protocol === 'http:' ? 'ws:' : 'wss:';
const socketAddress = `${socketProtocol}//${window.location.host}/ws`;
let socket;

let requestId = 1;

const appContainerEl = document.getElementById('app');
const statusEl = document.getElementById('status');
const reconnectButtonEl = document.getElementById('reconnect-button');
const moduleStatusElements = {};
const moduleMessageElements = {};

const heartbeat = () => {
    clearTimeout(heartbeatTimeout);
    heartbeatTimeout = setTimeout(() => {
        console.log('connection timed out');
        socket.close();
        statusEl.innerText = 'Disconnected';
        initReconnect();
    }, CONNECTION_TIMEOUT_MS);
};

const cancelHeartbeat = () => {
    clearTimeout(heartbeatTimeout);
};

const callAction = (pluginInstance, module, action) => {
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

const connect = () => {
    console.log('connect');
    socket = new WebSocket(socketAddress);

    socket.addEventListener('open', () => {
        console.log('socket open');
        reconnecting = false;
        clearTimeout(reconnectTimeout);
        reconnectButtonEl.classList.add('hidden');
        if (initialConnect) {
            initialConnect = false;
            reconnectButtonEl.innerText = 'Reconnect';
        }
        statusEl.innerText = 'Loading server state';
    });

    socket.addEventListener('error', (event) => {
        console.log('socket error', event);

        if (initialConnect) {
            statusEl.innerText = 'Connection failed';
            if (!reconnecting) {
                initReconnect();
            }
        } else {
            statusEl.innerText = 'Disconnected';
        }

        if (reconnecting) {
            reconnecting = false;
            scheduleReconnect();
        }

        cancelHeartbeat();
    });

    socket.addEventListener('message', (event) => {
        console.log('socket message:', event.data);

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
            const containerEl = document.createElement('div');
            containerEl.classList.add('modules');
            for (const pluginInstance of msg.system_info.plugin_instances) {
                for (const module of pluginInstance.modules) {
                    const moduleIdentifier = `${pluginInstance.name}::${module.name}`;

                    const moduleContainerEl = document.createElement('div');
                    moduleContainerEl.classList.add('module-container');
                    containerEl.appendChild(moduleContainerEl);

                    const moduleInfoEl = document.createElement('div');
                    moduleInfoEl.classList.add('module-info');
                    moduleContainerEl.appendChild(moduleInfoEl);

                    const moduleNameEl = document.createElement('span');
                    moduleNameEl.innerText = module.name;
                    moduleNameEl.classList.add('module-info__name');
                    moduleInfoEl.appendChild(moduleNameEl);

                    const moduleStatusEl = document.createElement('span');
                    moduleStatusEl.innerText = module.status;
                    moduleStatusEl.classList.add('module-info__status');
                    moduleStatusElements[moduleIdentifier] = moduleStatusEl;
                    moduleInfoEl.appendChild(moduleStatusEl);

                    const moduleActionsEl = document.createElement('div');
                    moduleActionsEl.classList.add('module-actions');
                    moduleContainerEl.appendChild(moduleActionsEl);
                    for (const action of module.actions) {
                        const actionButtonEl = document.createElement('button');
                        actionButtonEl.classList.add('action-button');
                        actionButtonEl.innerText = action;
                        actionButtonEl.onclick = () => {
                            console.log(`call ${moduleIdentifier}::${action}`);
                            callAction(pluginInstance.name, module.name, action);
                        };
                        moduleActionsEl.appendChild(actionButtonEl);
                    }

                    const moduleMessageEl = document.createElement('div');
                    moduleMessageEl.classList.add('module-message');
                    if (module.message) {
                        moduleMessageEl.innerHTML = module.message;
                    }
                    moduleMessageElements[moduleIdentifier] = moduleMessageEl;
                    moduleContainerEl.appendChild(moduleMessageEl);
                }
            }
            appContainerEl.innerHTML = '';
            appContainerEl.appendChild(containerEl);
            statusEl.innerText = 'Connected';
        } else if (msg.notification) {
            const notification = msg.notification;
            if (notification.module_status_update) {
                const update = notification.module_status_update;
                const identifier = `${update.plugin_instance}::${update.module}`;
                moduleStatusElements[identifier].innerText = update.new_status;
            } else if (notification.module_message_update) {
                const update = notification.module_message_update;
                const identifier = `${update.plugin_instance}::${update.module}`;
                moduleMessageElements[identifier].innerHTML = update.new_message;
            }
        }
    });
};

const tryReconnect = () => {
    // For manual reconnecting
    clearTimeout(reconnectTimeout);
    if (initialConnect) {
        statusEl.innerText = 'Connecting';
    } else {
        statusEl.innerText = 'Reconnecting';
    }
    reconnecting = true;
    reconnectButtonEl.classList.add('hidden');
    connect();
};

const scheduleReconnect = () => {
    reconnectButtonEl.classList.remove('hidden');
    reconnectTry++;
    let reconnectIntervalIdx = reconnectTry - 1;
    if (reconnectIntervalIdx >= RECONNECT_INTERVALS_MS.length) {
        reconnectIntervalIdx = RECONNECT_INTERVALS_MS.length - 1;
    }
    reconnectTimeout = setTimeout(tryReconnect, RECONNECT_INTERVALS_MS[reconnectIntervalIdx]);
};

const initReconnect = () => {
    reconnectTry = 0;
    scheduleReconnect();
};

reconnectButtonEl.addEventListener('click', tryReconnect);

statusEl.innerText = 'Connecting';
connect();
