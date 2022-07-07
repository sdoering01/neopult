-- load plugins here
require("channel_banner").setup({ resolution = { 1920, 1080 } })
require("cvh_camera").setup({
    camera_server_path = "../cvh-camera/camera-server/dist/server.js",
    sender_base_url = "http://localhost:3000/camera-sender.html",
    cameras = 2,
    janus_room_pin = "testcvh",
})
require("vnc").setup()
-- require("xapps").setup()
-- require("output").setup()
