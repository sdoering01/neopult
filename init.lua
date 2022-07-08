-- load plugins here
local channel = neopult.api.get_channel()

require("channel_banner").setup({ resolution = { 1920, 1080 } })
require("cvh_camera").setup({
    camera_server_path = "../cvh-camera/camera-server/dist/server.js",
    sender_base_url = "http://localhost:3000/camera-sender.html",
    cameras = 2,
    janus_room_pin = "testcvh",
})
require("vnc").setup({ listen = 2 * channel })
require("vnc").setup({ listen = 2 * channel + 1 })
-- require("xapps").setup()
-- require("output").setup()
