-- Idrisi's first OSM flex-output profile.
-- Geometry is stored in WGS84 (EPSG:4326); Martin reprojects to Web Mercator for MVT.
-- This is a deliberately small base-map schema, not a complete cartographic style.

local roads = osm2pgsql.define_way_table('osm_roads', {
    { column = 'highway', type = 'text' },
    { column = 'name', type = 'text' },
    { column = 'ref', type = 'text' },
    { column = 'surface', type = 'text' },
    { column = 'access', type = 'text' },
    { column = 'tags', type = 'jsonb' },
    { column = 'geom', type = 'linestring', projection = 4326 },
}, { schema = 'tiles' })

local places = osm2pgsql.define_node_table('osm_places', {
    { column = 'class', type = 'text' },
    { column = 'subtype', type = 'text' },
    { column = 'name', type = 'text' },
    { column = 'tags', type = 'jsonb' },
    { column = 'geom', type = 'point', projection = 4326 },
}, { schema = 'tiles' })

local buildings = osm2pgsql.define_area_table('osm_buildings', {
    { column = 'building', type = 'text' },
    { column = 'name', type = 'text' },
    { column = 'amenity', type = 'text' },
    { column = 'tags', type = 'jsonb' },
    { column = 'geom', type = 'geometry', projection = 4326 },
}, { schema = 'tiles' })

local water = osm2pgsql.define_area_table('osm_water', {
    { column = 'natural', type = 'text' },
    { column = 'water', type = 'text' },
    { column = 'landuse', type = 'text' },
    { column = 'name', type = 'text' },
    { column = 'tags', type = 'jsonb' },
    { column = 'geom', type = 'geometry', projection = 4326 },
}, { schema = 'tiles' })

local function has_any(tags, keys)
    for _, key in ipairs(keys) do
        if tags[key] then
            return key
        end
    end
    return nil
end

function osm2pgsql.process_node(object)
    local tags = object.tags
    local class = has_any(tags, { 'place', 'amenity', 'shop', 'tourism', 'leisure', 'office', 'healthcare' })
    if not class then
        return
    end

    places:insert({
        class = class,
        subtype = tags[class],
        name = tags.name,
        tags = tags,
        geom = object:as_point(),
    })
end

function osm2pgsql.process_way(object)
    local tags = object.tags

    if tags.highway then
        roads:insert({
            highway = tags.highway,
            name = tags.name,
            ref = tags.ref,
            surface = tags.surface,
            access = tags.access,
            tags = tags,
            geom = object:as_linestring(),
        })
    end

    if object.is_closed and (tags.building or tags.amenity == 'parking') then
        local geom = object:as_polygon()
        if not geom:is_null() then
            buildings:insert({
                building = tags.building,
                name = tags.name,
                amenity = tags.amenity,
                tags = tags,
                geom = geom,
            })
        end
    end

    if object.is_closed and (tags.natural == 'water' or tags.landuse == 'reservoir' or tags.water) then
        local geom = object:as_polygon()
        if not geom:is_null() then
            water:insert({
                natural = tags.natural,
                water = tags.water,
                landuse = tags.landuse,
                name = tags.name,
                tags = tags,
                geom = geom,
            })
        end
    end
end

function osm2pgsql.process_relation(object)
    local tags = object.tags
    if tags.type ~= 'multipolygon' and tags.type ~= 'boundary' then
        return
    end

    local geom = object:as_multipolygon()
    if geom:is_null() then
        return
    end

    if tags.building or tags.amenity == 'parking' then
        buildings:insert({
            building = tags.building,
            name = tags.name,
            amenity = tags.amenity,
            tags = tags,
            geom = geom,
        })
    end

    if tags.natural == 'water' or tags.landuse == 'reservoir' or tags.water then
        water:insert({
            natural = tags.natural,
            water = tags.water,
            landuse = tags.landuse,
            name = tags.name,
            tags = tags,
            geom = geom,
        })
    end
end
