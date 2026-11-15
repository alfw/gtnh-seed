package fakemain; import cpw.mods.fml.common.event.FMLServerStartingEvent; import net.minecraft.command.*; import net.minecraft.server.dedicated.DedicatedServer;
public class Main { public static void main(String[] a) throws Exception {
  FMLServerStartingEvent e = new FMLServerStartingEvent(); new rwg2dump.Rwg2Dump().serverStarting(e);
  ICommand c = e.cmd; ICommandSender s = new DedicatedServer();
  System.out.println("name="+c.func_71517_b()+" usage="+c.func_71518_a(s)+" allowed="+c.func_71519_b(s)+" aliases="+c.func_71514_a()+" cmp="+c.compareTo(c));
  c.func_71515_b(s, new String[]{"987654321","My Seed"});
  c.func_71515_b(s, new String[]{});
  for (Thread t : Thread.getAllStackTraces().keySet()) if (t.getName().equals("rwg2dump")) t.join(); } }
