const std = @import("std");

pub const commands = @import("commands.zig");
pub const vm = @import("vm.zig");
pub const container = @import("container.zig");
pub const config = @import("config.zig");
pub const logger = @import("logger.zig");

pub const NovaError = error{
    VMNotFound,
    ContainerNotFound,
    InvalidConfig,
    SystemCommandFailed,
    PermissionDenied,
};

pub const InstanceType = enum {
    vm,
    container,
};

pub const Instance = struct {
    name: []const u8,
    type: InstanceType,
    pid: ?std.process.Child.Id,
    status: Status,
    
    pub const Status = enum {
        stopped,
        starting,
        running,
        stopping,
        error_state,
    };
};

test "basic nova functionality" {
    const testing = std.testing;
    
    const instance = Instance{
        .name = "test",
        .type = .vm,
        .pid = null,
        .status = .stopped,
    };
    
    try testing.expect(std.mem.eql(u8, instance.name, "test"));
    try testing.expect(instance.type == .vm);
    try testing.expect(instance.status == .stopped);
}