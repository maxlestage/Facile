-- 16 requests sent at once on each connection (HTTP pipelining).
init = function(args)
  local depth = tonumber(args[1]) or 16
  local r = {}
  for i = 1, depth do r[i] = wrk.format() end
  req = table.concat(r)
end
request = function() return req end
