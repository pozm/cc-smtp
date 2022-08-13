-- functions 

function get_url_file_name(url)
    return url:sub(url:len() - url:reverse():find("/")+2)
end
local in_cache = {}
function fetch_caches() 

    in_cache = fs.list("/.cache/")
    return in_cache

end
function fetch_cache(name) 

    local f = fs.open("/.cache/"..name, "r")
    if f then
        local contents = f.readAll()
        f.close()
        return contents
    else 
        return nil;
    end

end

function load_remote_content(url,cache_disable) 
    local content_name = get_url_file_name(url);

    local content =  fetch_cache(content_name);
    if not content then
        
        local r = http.get(url)
        if r then
            content = r.readAll()
            r.close()
            if not cache_disable then
                fs.makeDir("/.cache/")
                local f = fs.open("/.cache/"..content_name,"w")
                f.write(content)
                f.close()
            end
        else
            error("unable to load " .. url)
        end
    end

    local ok, lib = pcall(function() return loadstring(content)() end)
    if not ok then
        error("unable to load " .. url)
        fs.delete("/.cache/"..content_name)
    end

    return lib

end



-- start of code

local json = load_remote_content("https://github.com/rxi/json.lua/raw/master/json.lua")

local connect_url, expose_dir = ...
local ws = http.websocket(connect_url);
if not ws then error("unable to connect to " .. connect_url) end
local wake = ws.receive(25)
if wake == nil then
    error("Failed to connect to " .. connect_url)
    return
end

ws.send(json.encode({c=0,d={String=os.getComputerID().."-"..(os.getComputerLabel() or "unknown")}}))


function get_all_content(path) 

    local file_to_content = {}
    local files = fs.list(path)
    for i,v in next,files do 

        local pathx = path..v

        if fs.isDir(pathx) then 
            -- file_to_content[v] = get_all_content(pathx)
            -- table.insert(file_to_content,get_all_content(pathx))
            -- file_to_content[v] = get_all_content(pathx)
            ws.send(json.encode({c=1,d={FsObject={path=pathx,ftype=1,content=""}}}))
            get_all_content(pathx.."/")
        else
            -- file_to_content[v] = ""
            -- table.insert(file_to_content,v)
            local f = fs.open(pathx,"r")
            local contents = ""
            if f then
                contents = f.readAll()
                f.close()
                file_to_content[v] = ""
            end
            ws.send(json.encode({c=1,d={FsObject={path=pathx,ftype=0,content=contents}}}))
        end
    
    
    
    end

    return file_to_content

end


-- local files = json.encode({c=1,d={Object=get_all_content("/")}})

-- print({c=1,d=ws.send(files)})

get_all_content(expose_dir or "/");

ws.send(json.encode({c=2,d={String=""}}))

while true do 
    local content = ws.receive(16000)
    if not content then 
    
        print("dc")
        break
    
    end
    print(content)
    local jsn = json.decode(content)

    if jsn.c == 1 then 
        local p = jsn.d.FsObject.path
        local content = jsn.d.FsObject.content

        local f = fs.open(p,"w")
        if f then
            f.write(content)
            f.close()
        end
        print("wrote " .. p)

    elseif jsn.c == 9 then
        ws.send(json.encode({c=9,d={String=""}}))
        print("sent pong!")
    end
end