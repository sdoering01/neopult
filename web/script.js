const webSocketProtocol = window.location.protocol === 'http:' ? 'ws:' : 'wss:';
const socket = new WebSocket(`${webSocketProtocol}//${window.location.host}/ws`);

let requestId = 1;

const appContainerEl = document.getElementById('app');
const moduleStatusElements = {};
const moduleMessageElements = {};

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

socket.addEventListener('open', () => {
    console.log('socket open');
});

socket.addEventListener('error', (event) => {
    console.log('socket error', event);
});

socket.addEventListener('message', (event) => {
    console.log('websocket message:', event.data);

    let msg;
    try {
        msg = JSON.parse(event.data);
    } catch (e) {
        console.error('parse error:', e);
        return;
    }

    if (msg == 'ping') {
        // TODO: Set heartbeat
        socket.send('"pong"');
    } else if (msg == 'pong') {
        // TODO: Set heartbeat
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
            appContainerEl.appendChild(containerEl);
        }
    } else if (msg.notification) {
        const notification = msg.notification;
        if (notification.module_status_update) {
            const update = notification.module_status_update;
            const identifier = `${update.plugin_instance}::${update.module}`;
            moduleStatusElements[identifier].innerText = update.new_status;
        } else if (notification.module_message_update) {
            const update = notification.module_message_update;
            const identifier = `${update.plugin_instance}::${update.module}`;
            // const newMessage = update.new_message || '';
            // Hide message field maybe
            moduleMessageElements[identifier].innerHTML = update.new_message;
        }
    }
});
