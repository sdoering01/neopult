local config = neopult.config
-- Change this to your own password
config.websocket_password = "neopult"

local channel = neopult.api.get_channel()
local camera_mode_store = neopult.api.create_store()

require("channel_banner").setup({ resolution = { 1920, 1080 } })
require("camera_mode").setup({ store = camera_mode_store })
require("cvh_camera").setup({
    -- You might have to change this
    camera_server_path = "/usr/local/share/cvh-camera/camera-server/dist/server.js",
    -- Change this to match the URL where the camera sender is served
    sender_base_url = "https://your-domain.com/camera-sender.html",
    cameras = 2,
    -- Has to match the view-only password of the vnc channel and has to be public
    janus_room_pin = "testcvh",
    -- This has to match the admin key in the general block of janus.plugin.videoroom.jcfg and should REMAIN PRIVATE
    janus_admin_key = "secret",
    -- Allows users to specify a name that is displayed in their camera feed
    allow_custom_names = false,
    camera_mode_store = camera_mode_store,
})

-- NOTE: Remember to allow tcp traffic on port 5500 + `listen` in your firewall
-- Change this to match the URL/IP to which VNC clients can connect. `:<port>` will be appended to generate the connection url.
local listen_base_url = "your-domain.com"
local websockify_port_1 = 6180 + channel -- 6180 = 6080 + 100 (100 channels with one websockify instance each)
require("vnc").setup({
    listen = 2 * channel,
    listen_displayed = 1,
    listen_base_url = listen_base_url,
    camera_mode_store = camera_mode_store,
    -- Adds support for the yesvnc web connector.
    yesvnc = {
        -- Change this to match the URL where the yesvnc web connector is served
        interface_base_url = "https://your_domain.com/yesvnc/index.html",
        secure_websockify_connection = true,
        websockify_port = websockify_port_1,
        -- Change this to the host where the websockify instance is running
        websockify_host = "your_domain.com:" .. websockify_port_1,
        -- Change this to the path where the websockify instance is running
        websockify_path = "/",
    },
})
require("vnc").setup({
    listen = 2 * channel + 1,
    listen_displayed = 2,
    listen_base_url = listen_base_url,
    camera_mode_store = camera_mode_store,
})
