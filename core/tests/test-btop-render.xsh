use core.lib.btop_render as render
use core.lib.btop_replay as replay
use core.tests.support.btop as fixture

test test_btop_g1_screen_places_all_panel_values_at_80_by_24 {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(
    bundle,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}],
  )
  let frames = replay.models(replay.load(bundle.host_path()?)?)
  let screen = render.render(frames[1], 80, 24)

  assert ! screen.too_small
  assert screen.lines.len() == 24
  assert "btop" in screen.lines[0]
  assert "Usage 40.0%" in screen.lines[3]
  assert "Total 1.00 GiB" in (screen.lines[3].split("") |> drop(40).join(""))
  assert "Used 768.00 MiB" in (screen.lines[5].split("") |> drop(40).join(""))
  assert "Interface eth0" in screen.lines[14]
  assert "RX 10.00 KiB/s" in screen.lines[15]
  assert "PID UID CPU% RSS COMMAND" in (screen.lines[14].split("") |> drop(40).join(""))
  assert "42 1000 12.5% 64.00 MiB sample worker" in (screen.lines[15].split("") |> drop(40).join(""))
}

test test_btop_g1_screen_places_all_panel_values_at_120_by_40 {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(
    bundle,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}],
  )
  let frames = replay.models(replay.load(bundle.host_path()?)?)
  let screen = render.render(frames[1], 120, 40)

  assert ! screen.too_small
  assert screen.lines.len() == 40
  assert "Usage 40.0%" in screen.lines[3]
  assert "Total 1.00 GiB" in (screen.lines[3].split("") |> drop(60).join(""))
  assert "RX 10.00 KiB/s" in screen.lines[23]
  assert "42 1000 12.5%" in (screen.lines[23].split("") |> drop(60).join(""))
}

test test_btop_g1_screen_reports_safe_minimum_dimensions {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(bundle, [])
  let frames = replay.models(replay.load(bundle.host_path()?)?)
  let screen = render.render(frames[1], 79, 24)

  assert screen.too_small
  assert screen.lines.len() == 1
  assert "terminal too small (minimum 80x24)" in screen.lines[0]
  assert screen.lines[0].count_chars() <= 79
}
