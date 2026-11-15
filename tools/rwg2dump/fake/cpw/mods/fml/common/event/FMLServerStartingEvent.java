package cpw.mods.fml.common.event; import net.minecraft.command.ICommand;
public class FMLServerStartingEvent { public ICommand cmd; public ICommand registerServerCommand(ICommand c){cmd=c; return c;} }
