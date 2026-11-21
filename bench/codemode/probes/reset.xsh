let marker = p"/work/.left-by-an-earlier-run"
let seen = marker.exists()?
marker.write("x")
let names = fs.children(p"/work")? |> map .name |> sort()
print f"marker already present: {seen}; entries: {names.join(" ")}"
