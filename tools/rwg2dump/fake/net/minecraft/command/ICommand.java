package net.minecraft.command; import java.util.List;
public interface ICommand extends Comparable {
  String func_71517_b(); String func_71518_a(ICommandSender s); List func_71514_a();
  void func_71515_b(ICommandSender s, String[] a); boolean func_71519_b(ICommandSender s);
  List func_71516_a(ICommandSender s, String[] a); boolean func_82358_a(String[] a, int i); }
