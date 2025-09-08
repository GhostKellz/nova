const std = @import("std");
const jaguar = @import("jaguar");
const nova = @import("nova");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    // Initialize Nova GUI
    var app = try jaguar.App.init(.{
        .title = "Nova - Virtualization Manager",
        .width = 1024,
        .height = 768,
        .resizable = true,
        .allocator = allocator,
    });
    defer app.deinit();

    nova.logger.info("Starting Nova GUI v0.1.0", .{});
    
    // Setup the main window UI
    try setupMainWindow(&app);

    // Run the GUI event loop
    try app.run();
}

fn setupMainWindow(app: *jaguar.App) !void {
    // Create the main layout using Jaguar's widget system
    // This will be expanded as we understand the API better
    
    nova.logger.info("Setting up Nova main window", .{});
    
    // For now, just ensure the window can be created and displayed
    // We'll add widgets and layout in the next iteration
    _ = app;
}

// Handle window events and UI interactions
pub fn handleVMAction(action: VMAction, name: []const u8) !void {
    const allocator = std.heap.page_allocator;
    
    switch (action) {
        .start => {
            nova.logger.info("GUI: Starting VM {s}", .{name});
            try nova.commands.run(allocator, "vm", name);
        },
        .stop => {
            nova.logger.info("GUI: Stopping VM {s}", .{name});
            try nova.commands.stop(allocator, "vm", name);
        },
        .status => {
            nova.logger.info("GUI: Checking VM {s} status", .{name});
            try nova.commands.list(allocator);
        },
    }
}

pub fn handleContainerAction(action: ContainerAction, name: []const u8) !void {
    const allocator = std.heap.page_allocator;
    
    switch (action) {
        .start => {
            nova.logger.info("GUI: Starting container {s}", .{name});
            try nova.commands.run(allocator, "container", name);
        },
        .stop => {
            nova.logger.info("GUI: Stopping container {s}", .{name});
            try nova.commands.stop(allocator, "container", name);
        },
        .status => {
            nova.logger.info("GUI: Checking container {s} status", .{name});
            try nova.commands.list(allocator);
        },
    }
}

const VMAction = enum {
    start,
    stop,
    status,
};

const ContainerAction = enum {
    start,
    stop,
    status,
};