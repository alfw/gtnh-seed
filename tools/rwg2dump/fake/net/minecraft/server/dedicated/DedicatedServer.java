package net.minecraft.server.dedicated; import net.minecraft.command.ICommandSender; import net.minecraft.util.IChatComponent;
public class DedicatedServer implements ICommandSender { public void func_145747_a(IChatComponent c){System.out.println("CHAT: "+c);} public String func_70005_c_(){return "Server";} }
