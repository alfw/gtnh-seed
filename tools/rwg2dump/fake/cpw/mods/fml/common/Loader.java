package cpw.mods.fml.common; import java.util.*;
public class Loader { public static Loader instance(){return new Loader();} public List<C> getModList(){return Arrays.asList(new C());}
  public static class C { public String getModId(){return "rwg2";} public String getVersion(){return "2.0.6";} public Object getSource(){return new java.io.File("rwg2-2.0.6.jar");} } }
