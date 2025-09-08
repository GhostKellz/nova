const std = @import("std");
const nova = @import("nova");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    nova.logger.info("Starting Nova GUI Demo v0.1.0", .{});

    // Demo Nova GUI interface (console version for testing)
    try runGUIDemo(allocator);
}

fn runGUIDemo(allocator: std.mem.Allocator) !void {
    std.debug.print("\n", .{});
    std.debug.print("╔══════════════════════════════════════════════════════════════╗\n", .{});
    std.debug.print("║                     🚀 Nova GUI v0.1.0                      ║\n", .{});
    std.debug.print("║              Wayland-Native Virtualization Manager           ║\n", .{});
    std.debug.print("╠══════════════════════════════════════════════════════════════╣\n", .{});
    std.debug.print("║                                                              ║\n", .{});
    
    // Display VM status
    std.debug.print("║  💻 Virtual Machines                                         ║\n", .{});
    try displayVMStatus(allocator);
    
    std.debug.print("║                                                              ║\n", .{});
    
    // Display Container status  
    std.debug.print("║  🐳 Containers                                               ║\n", .{});
    try displayContainerStatus(allocator);
    
    std.debug.print("║                                                              ║\n", .{});
    std.debug.print("║  📊 System Status                                            ║\n", .{});
    std.debug.print("║    ├─ Nova Core: ✅ Active                                   ║\n", .{});
    std.debug.print("║    ├─ QEMU/KVM: ✅ Available                                 ║\n", .{});
    std.debug.print("║    ├─ Containers: ✅ Available                               ║\n", .{});
    std.debug.print("║    └─ Wayland GUI: 🚧 Development                           ║\n", .{});
    std.debug.print("║                                                              ║\n", .{});
    std.debug.print("║  🎯 Available Actions:                                       ║\n", .{});
    std.debug.print("║    • Start/Stop VMs and Containers                          ║\n", .{});
    std.debug.print("║    • Monitor resource usage                                 ║\n", .{});
    std.debug.print("║    • Manage NovaFile configurations                         ║\n", .{});
    std.debug.print("║    • Network and storage management                         ║\n", .{});
    std.debug.print("║                                                              ║\n", .{});
    std.debug.print("╚══════════════════════════════════════════════════════════════╝\n", .{});
    std.debug.print("\n", .{});
    
    nova.logger.info("GUI Demo completed successfully", .{});
    nova.logger.info("Next: Implement wzl Wayland windows and Jaguar widgets", .{});
}

fn displayVMStatus(allocator: std.mem.Allocator) !void {
    // In a real GUI, this would query the actual VM status
    // For demo purposes, show some sample data
    _ = allocator;
    
    std.debug.print("║    ├─ win11-dev      [STOPPED]  8GB RAM, 4 CPU             ║\n", .{});
    std.debug.print("║    ├─ ubuntu-test    [RUNNING]  2GB RAM, 2 CPU             ║\n", .{});
    std.debug.print("║    └─ arch-build     [STOPPED]  4GB RAM, 8 CPU             ║\n", .{});
}

fn displayContainerStatus(allocator: std.mem.Allocator) !void {
    // In a real GUI, this would query the actual container status
    // For demo purposes, show some sample data
    _ = allocator;
    
    std.debug.print("║    ├─ web-api        [RUNNING]  nginx:latest                ║\n", .{});
    std.debug.print("║    ├─ database       [RUNNING]  postgres:15                 ║\n", .{});
    std.debug.print("║    └─ redis-cache    [STOPPED]  redis:alpine               ║\n", .{});
}

// Future integration points for real GUI
pub fn startVMFromGUI(name: []const u8) !void {
    const allocator = std.heap.page_allocator;
    nova.logger.info("GUI: Starting VM {s}", .{name});
    try nova.commands.run(allocator, "vm", name);
}

pub fn stopVMFromGUI(name: []const u8) !void {
    const allocator = std.heap.page_allocator;
    nova.logger.info("GUI: Stopping VM {s}", .{name});
    try nova.commands.stop(allocator, "vm", name);
}

pub fn startContainerFromGUI(name: []const u8) !void {
    const allocator = std.heap.page_allocator;
    nova.logger.info("GUI: Starting container {s}", .{name});
    try nova.commands.run(allocator, "container", name);
}

pub fn stopContainerFromGUI(name: []const u8) !void {
    const allocator = std.heap.page_allocator;
    nova.logger.info("GUI: Stopping container {s}", .{name});
    try nova.commands.stop(allocator, "container", name);
}