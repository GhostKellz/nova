const std = @import("std");
const nova = @import("root.zig");

// Simple global instance storage for v0.1.0
var instances: [16]nova.Instance = undefined;
var instance_count: usize = 0;

pub fn run(allocator: std.mem.Allocator, instance_type: []const u8, name: []const u8) !void {
    nova.logger.info("Starting {s} '{s}'", .{ instance_type, name });

    if (std.mem.eql(u8, instance_type, "vm")) {
        try nova.vm.start(allocator, name);
    } else if (std.mem.eql(u8, instance_type, "container")) {
        try nova.container.start(allocator, name);
    } else {
        std.debug.print("Error: Unknown instance type '{s}'. Use 'vm' or 'container'\n", .{instance_type});
        return nova.NovaError.InvalidConfig;
    }

    if (instance_count < instances.len) {
        instances[instance_count] = nova.Instance{
            .name = try allocator.dupe(u8, name),
            .type = if (std.mem.eql(u8, instance_type, "vm")) .vm else .container,
            .pid = null, // TODO: capture actual PID
            .status = .starting,
        };
        instance_count += 1;
    }
    nova.logger.info("{s} '{s}' started successfully", .{ instance_type, name });
}

pub fn stop(allocator: std.mem.Allocator, instance_type: []const u8, name: []const u8) !void {
    nova.logger.info("Stopping {s} '{s}'", .{ instance_type, name });

    if (std.mem.eql(u8, instance_type, "vm")) {
        try nova.vm.stop(allocator, name);
    } else if (std.mem.eql(u8, instance_type, "container")) {
        try nova.container.stop(allocator, name);
    } else {
        std.debug.print("Error: Unknown instance type '{s}'. Use 'vm' or 'container'\n", .{instance_type});
        return nova.NovaError.InvalidConfig;
    }

    // Update registry
    for (instances[0..instance_count]) |*instance| {
        if (std.mem.eql(u8, instance.name, name)) {
            instance.status = .stopped;
            break;
        }
    }
    
    nova.logger.info("{s} '{s}' stopped successfully", .{ instance_type, name });
}

pub fn list(allocator: std.mem.Allocator) !void {
    _ = allocator;
    nova.logger.info("Listing all instances", .{});
    
    std.debug.print("NAME\t\tTYPE\t\tSTATUS\t\tPID\n", .{});
    std.debug.print("----\t\t----\t\t------\t\t---\n", .{});
    
    if (instance_count == 0) {
        std.debug.print("No instances running\n", .{});
        return;
    }

    for (instances[0..instance_count]) |instance| {
        const type_str = if (instance.type == .vm) "VM" else "Container";
        const status_str = switch (instance.status) {
            .stopped => "STOPPED",
            .starting => "STARTING",
            .running => "RUNNING",
            .stopping => "STOPPING",
            .error_state => "ERROR",
        };
        
        const pid_str = if (instance.pid) |pid| pid else 0;
        
        std.debug.print("{s}\t\t{s}\t\t{s}\t\t{d}\n", .{ instance.name, type_str, status_str, pid_str });
    }
}