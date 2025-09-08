const std = @import("std");
const nova = @import("root.zig");

pub const NovaConfig = struct {
    project: ?[]const u8 = null,
    vms: std.StringHashMap(VMConfig),
    containers: std.StringHashMap(ContainerConfig),
    networks: std.StringHashMap(NetworkConfig),
    
    pub fn init(allocator: std.mem.Allocator) NovaConfig {
        return NovaConfig{
            .vms = std.StringHashMap(VMConfig).init(allocator),
            .containers = std.StringHashMap(ContainerConfig).init(allocator),
            .networks = std.StringHashMap(NetworkConfig).init(allocator),
        };
    }
    
    pub fn deinit(self: *NovaConfig) void {
        self.vms.deinit();
        self.containers.deinit();
        self.networks.deinit();
    }
};

pub const VMConfig = struct {
    image: ?[]const u8 = null,
    cpu: u32 = 2,
    memory: []const u8 = "1Gi",
    gpu_passthrough: bool = false,
    network: ?[]const u8 = null,
};

pub const ContainerConfig = struct {
    capsule: ?[]const u8 = null,
    volumes: [][]const u8 = &.{},
    network: ?[]const u8 = null,
    env: std.StringHashMap([]const u8),
    
    pub fn init(allocator: std.mem.Allocator) ContainerConfig {
        return ContainerConfig{
            .env = std.StringHashMap([]const u8).init(allocator),
        };
    }
    
    pub fn deinit(self: *ContainerConfig) void {
        self.env.deinit();
    }
};

pub const NetworkConfig = struct {
    type: NetworkType = .bridge,
    interfaces: [][]const u8 = &.{},
    driver: ?[]const u8 = null,
    dns: bool = false,
    
    pub const NetworkType = enum {
        bridge,
        overlay,
        host,
    };
};

pub fn parseNovaFile(allocator: std.mem.Allocator, file_path: []const u8) !NovaConfig {
    nova.logger.debug("Parsing NovaFile: {s}", .{file_path});
    
    const file = std.fs.cwd().openFile(file_path, .{}) catch |err| {
        nova.logger.err("Failed to open NovaFile: {}", .{err});
        return err;
    };
    defer file.close();
    
    const contents = file.readToEndAlloc(allocator, 1024 * 1024) catch |err| {
        nova.logger.err("Failed to read NovaFile: {}", .{err});
        return err;
    };
    defer allocator.free(contents);
    
    // For v0.1.0, we'll do a simple line-by-line parser
    // Full TOML parsing will come later
    return parseSimpleTOML(allocator, contents);
}

fn parseSimpleTOML(allocator: std.mem.Allocator, contents: []const u8) !NovaConfig {
    var config = NovaConfig.init(allocator);
    var lines = std.mem.split(u8, contents, "\n");
    var current_section: ?[]const u8 = null;
    var current_vm_name: ?[]const u8 = null;
    var current_container_name: ?[]const u8 = null;
    
    nova.logger.debug("Parsing {} bytes of TOML content", .{contents.len});
    
    while (lines.next()) |line| {
        const trimmed = std.mem.trim(u8, line, " \t\r");
        if (trimmed.len == 0 or trimmed[0] == '#') continue;
        
        // Section headers like [vm.name] or [container.name]
        if (trimmed[0] == '[' and trimmed[trimmed.len - 1] == ']') {
            const section = trimmed[1 .. trimmed.len - 1];
            current_section = section;
            
            if (std.mem.startsWith(u8, section, "vm.")) {
                current_vm_name = section[3..];
                try config.vms.put(current_vm_name.?, VMConfig{});
            } else if (std.mem.startsWith(u8, section, "container.")) {
                current_container_name = section[10..];
                try config.containers.put(current_container_name.?, ContainerConfig.init(allocator));
            }
            continue;
        }
        
        // Key-value pairs
        if (std.mem.indexOf(u8, trimmed, " = ") != null) {
            var kv = std.mem.split(u8, trimmed, " = ");
            const key = std.mem.trim(u8, kv.next().?, " \"");
            const value = std.mem.trim(u8, kv.next().?, " \"");
            
            // For now, just log the key-value pairs
            nova.logger.debug("Found config: {s} = {s} (in section: {s})", .{ key, value, current_section orelse "none" });
        }
    }
    
    nova.logger.info("Parsed NovaFile with {} VMs, {} containers", .{ config.vms.count(), config.containers.count() });
    return config;
}