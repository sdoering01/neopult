(() => {
    'use strict';

    const LOCAL_STORAGE_PASSWORD_KEY = 'neopult_password';

    // NOTE: Make sure to adjust the timeout when changing the ping interval on the server
    const CONNECTION_TIMEOUT_MS = 10000;
    const RECONNECT_INTERVALS_MS = [1000, 3000, 10000];

    const RECONNECT_LABEL_INITIAL = 'Connect';
    const RECONNECT_LABEL = 'Reconnect';

    const SOCKET_DISCONNECT_REASON_AUTH = 'auth';
    const SOCKET_DISCONNECT_REASON_AUTH_TIMEOUT = 'auth_timeout';

    let initialConnect = true;
    let reconnecting = false;
    let heartbeatTimeout;
    let reconnectTry;
    let reconnectTimeout;
    let reconnectUpdateInterval;
    let password;
    let rememberPassword = false;
    let hasStoredPassword = false;

    const socketProtocol = window.location.protocol === 'http:' ? 'ws:' : 'wss:';
    let socketPath = window.location.pathname;
    if (!socketPath.endsWith('/')) {
        if (socketPath.endsWith('.html')) {
            const parts = socketPath.split('/');
            parts.pop();
            socketPath = parts.join('/');
        }
        socketPath += '/';
    }
    socketPath += 'ws';
    const socketAddress = `${socketProtocol}//${window.location.host}${socketPath}`;
    let socket;

    let requestId = 1;

    const authContainerEl = document.getElementById('auth-container');
    const passwordFormEl = document.getElementById('password-form');
    const passwordInputEl = document.getElementById('password-input');
    const passwordRememberCheckboxEl = document.getElementById('password-remember-checkbox');
    const passwordSendButtonEl = document.getElementById('password-send-button');
    const appContainerEl = document.getElementById('app-container');
    const logoutButtonEl = document.getElementById('logout-button');
    const pluginContainerEl = document.getElementById('plugin-container');
    const statusEl = document.getElementById('status');
    const reconnectButtonEl = document.getElementById('reconnect-button');
    const moduleStatusElements = {};
    const moduleMessageElements = {};

    const heartbeat = () => {
        clearTimeout(heartbeatTimeout);
        heartbeatTimeout = setTimeout(() => {
            console.log('connection timed out');
            disconnect();
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

        passwordSendButtonEl.disabled = true;
        passwordRememberCheckboxEl.disabled = true;

        if (initialConnect) {
            statusEl.innerText = 'Connecting';
        } else {
            statusEl.innerText = 'Reconnecting';
        }

        socket.addEventListener('open', handleSocketOpen);
        socket.addEventListener('error', handleSocketError);
        socket.addEventListener('close', handleSocketClose);
        socket.addEventListener('message', handleSocketMessage);
    };

    const handleSocketOpen = (event) => {
        console.log('socket open', event);
        reconnecting = false;
        initialConnect = false;
        clearTimeout(reconnectTimeout);
        clearInterval(reconnectUpdateInterval);
        reconnectButtonEl.classList.add('hidden');
        if (hasStoredPassword) {
            statusEl.innerText = 'Authenticating with stored password';
        } else {
            statusEl.innerText = 'Authenticating';
        }
        socket.send('Password ' + password);
    };

    const handleSocketMessage = (event) => {
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
            statusEl.innerText = 'Connected';

            if (rememberPassword) {
                localStorage.setItem(LOCAL_STORAGE_PASSWORD_KEY, password);
            }

            appContainerEl.classList.remove('hidden');
            authContainerEl.classList.add('hidden');

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
            pluginContainerEl.innerHTML = '';
            pluginContainerEl.appendChild(containerEl);
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
    };

    const handleSocketError = (event) => {
        console.log('socket error', event);
    };

    const handleSocketClose = (event) => {
        console.log('socket close', event);
        handleDisconnect(event.reason);
    };

    const disconnect = (reconnectOverwrite = null) => {
        socket.close();
        handleDisconnect('', reconnectOverwrite);
    };

    const handleDisconnect = (reason, reconnectOverwrite = null) => {
        socket.removeEventListener('open', handleSocketOpen);
        socket.removeEventListener('error', handleSocketError);
        socket.removeEventListener('close', handleSocketClose);
        socket.removeEventListener('message', handleSocketMessage);

        pluginContainerEl.innerHTML = '';
        cancelHeartbeat();

        let statusText;
        if (reason === SOCKET_DISCONNECT_REASON_AUTH) {
            if (hasStoredPassword) {
                localStorage.removeItem(LOCAL_STORAGE_PASSWORD_KEY);
                statusText = 'Stored password incorrect';
                hasStoredPassword = false;
            } else {
                statusText = 'Password incorrect';
            }
        } else if (reason === SOCKET_DISCONNECT_REASON_AUTH_TIMEOUT) {
            statusText = 'Socket authentication timed out';
        } else {
            if (initialConnect) {
                statusText = 'Connection failed';
            } else {
                statusText = 'Disconnected';
            }
        }
        statusEl.innerText = statusText;

        if (
            reconnectOverwrite === false ||
            reason === SOCKET_DISCONNECT_REASON_AUTH ||
            reason === SOCKET_DISCONNECT_REASON_AUTH_TIMEOUT
        ) {
            passwordSendButtonEl.disabled = false;
            passwordRememberCheckboxEl.disabled = false;
            appContainerEl.classList.add('hidden');
            authContainerEl.classList.remove('hidden');
        }

        const shouldReconnect =
            reconnectOverwrite != null
                ? reconnectOverwrite
                : reason !== SOCKET_DISCONNECT_REASON_AUTH &&
                  reason !== SOCKET_DISCONNECT_REASON_AUTH_TIMEOUT;
        if (shouldReconnect) {
            if (reconnecting) {
                reconnecting = false;
                scheduleReconnect();
            } else {
                initReconnect();
            }
        }
    };

    const reconnect = () => {
        // For manual reconnecting
        clearTimeout(reconnectTimeout);
        clearInterval(reconnectUpdateInterval);
        window.removeEventListener('focus', reconnect);
        reconnecting = true;
        reconnectButtonEl.classList.add('hidden');
        connect();
    };

    const updateReconnectButton = (millis) => {
        let label = initialConnect ? RECONNECT_LABEL_INITIAL : RECONNECT_LABEL;
        let secs = Math.ceil(millis / 1000);
        reconnectButtonEl.innerText = `${label} (retrying in ${secs})`;
    };

    const scheduleReconnect = () => {
        reconnectButtonEl.classList.remove('hidden');
        reconnectTry++;
        let reconnectIntervalIdx = reconnectTry - 1;
        if (reconnectIntervalIdx >= RECONNECT_INTERVALS_MS.length) {
            reconnectIntervalIdx = RECONNECT_INTERVALS_MS.length - 1;
        }
        let reconnectIn = RECONNECT_INTERVALS_MS[reconnectIntervalIdx];
        reconnectTimeout = setTimeout(reconnect, reconnectIn);
        updateReconnectButton(reconnectIn);
        reconnectUpdateInterval = setInterval(() => {
            reconnectIn -= 1000;
            updateReconnectButton(reconnectIn);
        }, 1000);
        window.addEventListener('focus', reconnect);
    };

    const initReconnect = () => {
        reconnectTry = 0;
        scheduleReconnect();
    };

    const handlePasswordSubmit = (event) => {
        event.preventDefault();
        password = passwordInputEl.value;
        rememberPassword = passwordRememberCheckboxEl.checked;
        passwordInputEl.value = '';
        connect();
    };

    const tryAutoConnect = () => {
        const storedPassword = localStorage.getItem(LOCAL_STORAGE_PASSWORD_KEY);
        if (storedPassword !== null) {
            hasStoredPassword = true;
            password = storedPassword;
            connect();
        }
    };

    const logout = () => {
        disconnect(false);
        localStorage.removeItem(LOCAL_STORAGE_PASSWORD_KEY);
        password = '';
        hasStoredPassword = false;
    };

    reconnectButtonEl.addEventListener('click', reconnect);
    passwordFormEl.addEventListener('submit', handlePasswordSubmit);
    logoutButtonEl.addEventListener('click', logout);

    tryAutoConnect();
})();
