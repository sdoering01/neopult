-- load plugins here
require("cvh_camera").setup({
    camera_server_path = "../cvh-camera/camera-server/dist/server.js",
    sender_base_url = "http://localhost:3000/camera-sender.html",
    cameras = 2
})
require("vnc").setup()
-- require("xapps").setup()
-- require("output").setup()
