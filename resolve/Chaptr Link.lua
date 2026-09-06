--[[
  chaptr link — jump the Resolve playhead to whatever is clicked in chaptr.

  Run it from Workspace > Scripts and leave it running. It watches a small file
  chaptr writes and moves the playhead to the matching frame of the matching
  clip on the current timeline.

  Lua rather than Python on purpose: Lua is built into Resolve, needs no
  install, and works on the free edition, where external scripting does not.
]]

local POLL = 0.15

local function home()
  return os.getenv("HOME") or os.getenv("USERPROFILE") or "."
end

-- Matches the path chaptr writes to. APPDATA on Windows, the XDG data dir
-- elsewhere, so this has to check both.
local function requestPath()
  local appdata = os.getenv("APPDATA")
  if appdata then return appdata .. "\\chaptr\\goto.txt" end
  return home() .. "/.local/share/chaptr/goto.txt"
end

local function read(path)
  local f = io.open(path, "r")
  if not f then return nil end
  local text = f:read("*a")
  f:close()
  return text
end

local function basename(p)
  return (p or ""):match("([^/\\]+)$") or ""
end

local function timecode(frame, fps)
  local rate = math.floor(fps + 0.5)
  if rate < 1 then rate = 24 end
  if frame < 0 then frame = 0 end
  local f = frame % rate
  local total = math.floor(frame / rate)
  local s = total % 60
  local m = math.floor(total / 60) % 60
  local h = math.floor(total / 3600)
  return string.format("%02d:%02d:%02d:%02d", h, m, s, f)
end

-- A recording can appear as several timeline items once it has been cut, and
-- the moment may have been trimmed out of some of them. Ask each item which
-- part of the source it actually shows.
local function findFrame(timeline, wanted, offset, fps)
  local target = math.floor(offset * fps + 0.5)
  for _, track in ipairs({ "video", "audio" }) do
    local count = timeline:GetTrackCount(track)
    for i = 1, count do
      for _, item in ipairs(timeline:GetItemListInTrack(track, i) or {}) do
        local pool = item:GetMediaPoolItem()
        if pool then
          local name = pool:GetClipProperty("File Name")
          if name == "" or name == nil then
            name = basename(pool:GetClipProperty("File Path"))
          end
          if basename(name) == wanted then
            local from = item:GetSourceStartFrame()
            local to = item:GetSourceEndFrame()
            if target >= from and target <= to then
              return item:GetStart() + (target - from)
            end
          end
        end
      end
    end
  end
  return nil
end

-- Resolve exposes no way to stop playback from a script, and a seek issued
-- while the timeline is rolling is sometimes ignored. Set it, check whether it
-- took, and try again for a moment before giving up.
local function seek(timeline, frame, fps)
  local want = timecode(frame, fps)
  for attempt = 1, 8 do
    timeline:SetCurrentTimecode(want)
    bmd.wait(0.04)
    local now = timeline:GetCurrentTimecode()
    if now == want then return true, attempt end
    -- While playing, the playhead moves on after landing, so anything close
    -- counts as having worked.
    local h, m, s, f = tostring(now):match("(%d+):(%d+):(%d+):(%d+)")
    if h then
      local rate = math.floor(fps + 0.5)
      local at = ((tonumber(h) * 60 + tonumber(m)) * 60 + tonumber(s)) * rate + tonumber(f)
      if math.abs(at - frame) < rate * 2 then return true, attempt end
    end
  end
  return false, 8
end

local resolve = Resolve()
local path = requestPath()
local last = nil

print("chaptr link watching " .. path)
print("leave this running, and click a chaptr in chaptr")

while true do
  local text = read(path)
  if text and text ~= last then
    last = text
    -- <counter> <tab> <file name> <tab> <seconds>
    local id, file, offset = text:match("^(%d+)\t([^\t]+)\t([%d%.]+)")
    if id and file and offset then
      local project = resolve:GetProjectManager():GetCurrentProject()
      local timeline = project and project:GetCurrentTimeline()
      if not timeline then
        print("no timeline open")
      else
        local fps = tonumber(timeline:GetSetting("timelineFrameRate")) or 30
        local frame = findFrame(timeline, basename(file), tonumber(offset), fps)
        if frame then
          local ok, tries = seek(timeline, frame, fps)
          if not ok then
            print("could not move the playhead to " .. timecode(frame, fps)
              .. " after " .. tries .. " tries - stop playback and click again")
          end
        else
          print("not on this timeline: " .. file .. " at " .. offset .. "s")
        end
      end
    end
  end
  bmd.wait(POLL)
end
