-- Snapshot of the comment buffer for tuicr to render.
local buffer = vim.g.tuicr_comment_buffer
if not buffer or not vim.api.nvim_buf_is_valid(buffer) then
  return vim.NIL
end

local mode = vim.api.nvim_get_mode()
local raw = mode.mode
local active = vim.api.nvim_get_current_buf() == buffer
local cursor = active and vim.api.nvim_win_get_cursor(0) or { 1, 0 }

-- Plain Normal mode: nothing pending, so Esc/Enter mean nothing to Neovim and
-- tuicr may claim them for cancel/save.
local plain_normal = raw == "n" and not mode.blocking and vim.v.count == 0 and vim.fn.state("mo") == ""

local cmdline = vim.NIL
if raw:sub(1, 1) == "c" then
  cmdline = vim.fn.getcmdtype() .. vim.fn.getcmdline()
end

-- Visual selection as { row, start_col, end_col } byte spans, end exclusive.
-- getregionpos() reports 1-based columns with an inclusive end byte.
local selection = {}
local visual_types = { v = "v", V = "V", ["\22"] = "\22", s = "v", S = "V", ["\19"] = "\22" }
local visual_type = visual_types[raw:sub(1, 1)]
if active and visual_type and vim.fn.exists("*getregionpos") == 1 then
  local ok, regions = pcall(vim.fn.getregionpos, vim.fn.getpos("v"), vim.fn.getpos("."), {
    type = visual_type,
    eol = true,
  })
  if ok then
    for _, region in ipairs(regions) do
      local first, last = region[1], region[2]
      selection[#selection + 1] = { first[2] - 1, first[3] - 1, last[3] }
    end
  end
end

return {
  lines = vim.api.nvim_buf_get_lines(buffer, 0, -1, false),
  row = cursor[1] - 1,
  col = cursor[2],
  mode = raw,
  plain_normal = plain_normal,
  cmdline = cmdline,
  selection = selection,
  recording = vim.fn.reg_recording(),
  active = active,
}
