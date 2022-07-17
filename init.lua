local config = neopult.config
config.websocket_password = "neopult"

local channel = neopult.api.get_channel()
local camera_mode_store = neopult.api.create_store()

require("channel_banner").setup({ resolution = { 1920, 1080 } })
require("camera_mode").setup({ store = camera_mode_store })
require("cvh_camera").setup({
    camera_server_path = "../cvh-camera/camera-server/dist/server.js",
    sender_base_url = "http://localhost:3000/camera-sender.html",
    cameras = 2,
    janus_room_pin = "testcvh",
    camera_mode_store = camera_mode_store,
})

local listen_base_url = "127.0.0.1"
local websockify_port_1 = 6900 + channel -- 6900 = 6800 + 100 (100 channels with one websockify instance each)
require("vnc").setup({
    listen = 2 * channel,
    listen_base_url = listen_base_url,
    camera_mode_store = camera_mode_store,
    yesvnc = {
        interface_base_url = "http://localhost:3001/index.html",
        secure_websockify_connection = false,
        websockify_port = websockify_port_1,
        websockify_host = "localhost:" .. websockify_port_1,
        websockify_path = "/",
    },
})
require("vnc").setup({
    listen = 2 * channel + 1,
    listen_base_url = listen_base_url,
    camera_mode_store = camera_mode_store,
})
