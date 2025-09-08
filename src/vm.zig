const std = @import("std");
const nova = @import("root.zig");

pub fn start(allocator: std.mem.Allocator, name: []const u8) !void {
    nova.logger.debug("Attempting to start VM: {s}", .{name});
    
    // Try to find a NovaFile config
    const config_path = try std.fmt.allocPrint(allocator, "NovaFile", .{});
    defer allocator.free(config_path);
    
    const config_file = std.fs.cwd().openFile(config_path, .{}) catch |err| switch (err) {
        error.FileNotFound => {
            nova.logger.warn("NovaFile not found, using defaults for VM '{s}'", .{name});
            return startDefaultVM(allocator, name);
        },
        else => return err,
    };
    defer config_file.close();
    
    nova.logger.info("Found NovaFile, parsing VM configuration for '{s}'", .{name});
    return startConfiguredVM(allocator, name, config_file);
}

pub fn stop(allocator: std.mem.Allocator, name: []const u8) !void {
    _ = allocator;
    nova.logger.info("Stopping VM: {s}", .{name});
    
    // For v0.1.0, we'll use a simple approach to find and kill QEMU processes
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const arena_allocator = arena.allocator();
    
    const cmd = try std.fmt.allocPrint(arena_allocator, "pkill -f 'qemu.*{s}'", .{name});
    
    var child = std.process.Child.init(&.{ "sh", "-c", cmd }, arena_allocator);
    const result = child.spawnAndWait() catch |err| {
        nova.logger.err("Failed to stop VM '{s}': {}", .{ name, err });
        return nova.NovaError.SystemCommandFailed;
    };
    
    switch (result) {
        .Exited => |code| if (code == 0) {
            nova.logger.info("VM '{s}' stopped successfully", .{name});
        } else {
            nova.logger.warn("VM '{s}' may not have been running (exit code: {d})", .{ name, code });
        },
        else => {
            nova.logger.err("Unexpected result when stopping VM '{s}'", .{name});
            return nova.NovaError.SystemCommandFailed;
        },
    }
}

fn startDefaultVM(allocator: std.mem.Allocator, name: []const u8) !void {
    nova.logger.info("Starting default VM configuration for '{s}'", .{name});
    
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    const arena_allocator = arena.allocator();
    
    // Default VM configuration - minimal setup for testing
    const qemu_args = [_][]const u8{
        "qemu-system-x86_64",
        "-name", name,
        "-m", "512M",
        "-cpu", "host",
        "-enable-kvm",
        "-display", "none",
        "-monitor", "none",
        "-serial", "null",
        "-daemonize",
        // Create a minimal disk image if none exists
        "-drive", "file=/tmp/nova_test.qcow2,format=qcow2,if=virtio",
    };
    
    // Create a test disk image if it doesn't exist
    createTestDisk(arena_allocator) catch |err| {
        nova.logger.warn("Could not create test disk: {}", .{err});
    };
    
    var child = std.process.Child.init(&qemu_args, arena_allocator);
    const result = child.spawnAndWait() catch |err| {
        nova.logger.err("Failed to start VM '{s}': {}", .{ name, err });
        nova.logger.err("Make sure QEMU is installed and KVM is available", .{});
        return nova.NovaError.SystemCommandFailed;
    };
    
    switch (result) {
        .Exited => |code| if (code == 0) {
            nova.logger.info("VM '{s}' started successfully in background", .{name});
        } else {
            nova.logger.err("VM '{s}' failed to start (exit code: {d})", .{ name, code });
            return nova.NovaError.SystemCommandFailed;
        },
        else => {
            nova.logger.err("Unexpected result when starting VM '{s}'", .{name});
            return nova.NovaError.SystemCommandFailed;
        },
    }
}

fn startConfiguredVM(allocator: std.mem.Allocator, name: []const u8, config_file: std.fs.File) !void {
    _ = allocator;
    _ = name;
    _ = config_file;
    nova.logger.info("Configured VM startup not yet implemented, falling back to default", .{});
    return nova.NovaError.InvalidConfig;
}

fn createTestDisk(allocator: std.mem.Allocator) !void {
    const disk_path = "/tmp/nova_test.qcow2";
    
    // Check if disk already exists
    if (std.fs.accessAbsolute(disk_path, .{})) |_| {
        return; // Disk already exists
    } else |_| {
        // Disk doesn't exist, create it
    }
    
    const qemu_img_args = [_][]const u8{
        "qemu-img", "create", "-f", "qcow2", disk_path, "1G",
    };
    
    var child = std.process.Child.init(&qemu_img_args, allocator);
    _ = child.spawnAndWait() catch {
        return; // Ignore errors for now
    };
}