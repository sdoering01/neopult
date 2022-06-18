const socket = new WebSocket('ws://localhost:4000/ws');
let requestId = 1;

const appContainerEl = document.getElementById('app');
const moduleStatusElements = {};

const callAction = (pluginInstance, module, action) => {
    const request = {
        request_id: requestId.toString(),
        body: {
            call_action: {
                plugin_instance: pluginInstance,
                module,
                action,
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

socket.addEventListener('message', (event) => {
    console.log('websocket message:', event.data);

    let msg;
    try {
        msg = JSON.parse(event.data);
    } catch (e) {
        console.error('parse error:', e);
        return;
    }

    if (msg.system_info) {
        const containerEl = document.createElement('div');
        containerEl.classList.add('modules');
        for (const pluginInstance of msg.system_info.plugin_instances) {
            for (const module of pluginInstance.modules) {
                const moduleContainerEl = document.createElement('div');
                moduleContainerEl.classList.add('module-container');

                const moduleInfoEl = document.createElement('div');
                moduleInfoEl.classList.add('module-info');

                const moduleNameEl = document.createElement('span');
                moduleNameEl.innerText = module.name;
                moduleNameEl.classList.add('module-info__name');
                const moduleStatusEl = document.createElement('span');
                moduleStatusEl.innerText = module.status;
                moduleStatusEl.classList.add('module-info__status');
                moduleStatusElements[`${pluginInstance.name}::${module.name}`] = moduleStatusEl;

                moduleInfoEl.append(moduleNameEl, moduleStatusEl);

                const moduleActionsEl = document.createElement('div');
                moduleActionsEl.classList.add('module-actions');
                moduleContainerEl.append(moduleInfoEl, moduleActionsEl);
                containerEl.appendChild(moduleContainerEl);
                for (const action of module.actions) {
                    const actionButtonEl = document.createElement('button');
                    actionButtonEl.classList.add('action-button');
                    actionButtonEl.innerText = action;
                    actionButtonEl.onclick = () => {
                        console.log(`call ${pluginInstance.name}::${module.name}::${action}`);
                        callAction(pluginInstance.name, module.name, action);
                    };
                    moduleActionsEl.appendChild(actionButtonEl);
                }
            }
            appContainerEl.appendChild(containerEl);
        }
    } else if (msg.notification) {
        const notification = msg.notification;
        if (notification.module_status_update) {
            const update = notification.module_status_update;
            const identifier = `${update.plugin_instance}::${update.module}`;
            moduleStatusElements[identifier].innerText = update.new_status;
        }
    }
});
