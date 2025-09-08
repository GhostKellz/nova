const std = @import("std");
const wzl = @import("wzl");
const nova = @import("nova");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    nova.logger.info("Starting Nova GUI v0.1.0 (Simple Wayland Client)", .{});

    // Try to create a basic Wayland client
    var client = wzl.Client.init(allocator, .{}) catch |err| {
        nova.logger.err("Failed to initialize Wayland client: {}", .{err});
        nova.logger.info("GUI requires a running Wayland compositor", .{});
        return;
    };
    defer client.deinit();

    nova.logger.info("Wayland client initialized successfully", .{});

    // Connect to the Wayland display
    client.connect() catch |err| {
        nova.logger.err("Failed to connect to Wayland display: {}", .{err});
        nova.logger.info("Make sure you're running under a Wayland compositor", .{});
        return;
    };

    nova.logger.info("Connected to Wayland display", .{});
    nova.logger.info("Nova GUI is ready! (Basic Wayland integration)", .{});

    // For now, just demonstrate that we can initialize Wayland
    std.debug.print("🚀 Nova GUI v0.1.0\n", .{});
    std.debug.print("├─ Wayland Client: Connected\n", .{});
    std.debug.print("├─ Backend: Nova CLI Integration\n", .{});
    std.debug.print("└─ Status: Ready for development\n", .{});

    // In the future, this will create windows and render the Nova interface
    // For now, we'll just run a simple loop to keep the connection alive
    var counter: u32 = 0;
    while (counter < 10) {
        std.Thread.sleep(std.time.ns_per_s); // Sleep for 1 second
        counter += 1;
        nova.logger.debug("GUI heartbeat: {d}/10", .{counter});
    }

    nova.logger.info("Nova GUI shutting down", .{});
}

// Future GUI functions will go here
pub fn updateVMStatus() !void {
    // This will refresh the VM status display in the GUI
    nova.logger.debug("Updating VM status display", .{});
}

pub fn updateContainerStatus() !void {
    // This will refresh the container status display in the GUI
    nova.logger.debug("Updating container status display", .{});
}