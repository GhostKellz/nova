const std = @import("std");
const nova = @import("root.zig");

pub fn start(allocator: std.mem.Allocator, name: []const u8) !void {
    nova.logger.debug("Attempting to start container: {s}", .{name});
    
    // Try to find a NovaFile config
    const config_path = try std.fmt.allocPrint(allocator, "NovaFile", .{});
    defer allocator.free(config_path);
    
    const config_file = std.fs.cwd().openFile(config_path, .{}) catch |err| switch (err) {
        error.FileNotFound => {
            nova.logger.warn("NovaFile not found, using defaults for container '{s}'", .{name});
            return startDefaultContainer(allocator, name);
        },
        else => return err,
    };
    defer config_file.close();
    
    nova.logger.info("Found NovaFile, parsing container configuration for '{s}'", .{name});
    return startConfiguredContainer(allocator, name, config_file);
}

pub fn stop(allocator: std.mem.Allocator, name: []const u8) !void {
    _ = allocator;
    nova.logger.info("Stopping container: {s}", .{name});
    
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const arena_allocator = arena.allocator();
    
    // For v0.1.0, look for process with container name
    const cmd = try std.fmt.allocPrint(arena_allocator, "pkill -f 'nova-container-{s}'", .{name});
    
    var child = std.process.Child.init(&.{ "sh", "-c", cmd }, arena_allocator);
    const result = child.spawnAndWait() catch |err| {
        nova.logger.err("Failed to stop container '{s}': {}", .{ name, err });
        return nova.NovaError.SystemCommandFailed;
    };
    
    switch (result) {
        .Exited => |code| if (code == 0) {
            nova.logger.info("Container '{s}' stopped successfully", .{name});
        } else {
            nova.logger.warn("Container '{s}' may not have been running (exit code: {d})", .{ name, code });
        },
        else => {
            nova.logger.err("Unexpected result when stopping container '{s}'", .{name});
            return nova.NovaError.SystemCommandFailed;
        },
    }
}

fn startDefaultContainer(allocator: std.mem.Allocator, name: []const u8) !void {
    nova.logger.info("Starting default container configuration for '{s}'", .{name});
    
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    const arena_allocator = arena.allocator();
    
    // Create a simple "container" using unshare for basic namespace isolation
    // This is a very basic implementation - real containers need much more
    const container_script = try std.fmt.allocPrint(arena_allocator,
        \\#!/bin/bash
        \\exec -a nova-container-{s} unshare --pid --fork --mount-proc sleep infinity
    , .{name});
    
    // Write the script to a temp file
    const script_path = try std.fmt.allocPrint(arena_allocator, "/tmp/nova_container_{s}.sh", .{name});
    
    const script_file = std.fs.createFileAbsolute(script_path, .{ .mode = 0o755 }) catch |err| {
        nova.logger.err("Failed to create container script: {}", .{err});
        return nova.NovaError.SystemCommandFailed;
    };
    defer script_file.close();
    
    script_file.writeAll(container_script) catch |err| {
        nova.logger.err("Failed to write container script: {}", .{err});
        return nova.NovaError.SystemCommandFailed;
    };
    
    // Run the container script in background
    const container_args = [_][]const u8{ "bash", script_path };
    
    var child = std.process.Child.init(&container_args, arena_allocator);
    child.stdin_behavior = .Ignore;
    child.stdout_behavior = .Ignore;
    child.stderr_behavior = .Ignore;
    
    child.spawn() catch |err| {
        nova.logger.err("Failed to start container '{s}': {}", .{ name, err });
        nova.logger.err("Make sure 'unshare' command is available", .{});
        return nova.NovaError.SystemCommandFailed;
    };
    
    // Don't wait - let it run in background
    nova.logger.info("Container '{s}' started successfully in background", .{name});
}

fn startConfiguredContainer(allocator: std.mem.Allocator, name: []const u8, config_file: std.fs.File) !void {
    _ = allocator;
    _ = name;
    _ = config_file;
    nova.logger.info("Configured container startup not yet implemented, falling back to default", .{});
    return nova.NovaError.InvalidConfig;
}