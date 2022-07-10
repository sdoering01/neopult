-- load plugins here
local channel = neopult.api.get_channel()

require("channel_banner").setup({ resolution = { 1920, 1080 } })
require("cvh_camera").setup({
    camera_server_path = "../cvh-camera/camera-server/dist/server.js",
    sender_base_url = "http://localhost:3000/camera-sender.html",
    cameras = 2,
    janus_room_pin = "testcvh",
})

local listen_base_url = "127.0.0.1"
require("vnc").setup({
    listen = 2 * channel,
    listen_base_url = listen_base_url
})
require("vnc").setup({
    listen = 2 * channel + 1,
    listen_base_url = listen_base_url
})
